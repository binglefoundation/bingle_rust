use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bingle_core::distributed_mutex::ModifiedLamportDistributedMutex;
use bingle_core::messages::types::{MutexRelease, MutexRequest, MutexResponse};

#[derive(Clone)]
pub struct TestNetwork {
    pub nodes: Arc<Mutex<HashMap<String, Option<Arc<ModifiedLamportDistributedMutex>>>>>,
    pub down: Arc<Mutex<Vec<String>>>,
}

impl TestNetwork {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(Mutex::new(HashMap::new())),
            down: Arc::new(Mutex::new(vec![])),
        }
    }

    #[allow(dead_code)]
    pub fn is_down(&self, id: &str) -> bool {
        let d = self.down.lock().expect("down lock");
        d.iter().any(|x| x == id)
    }

    #[allow(dead_code)]
    pub fn drop_node(&self, id: &str) {
        self.down.lock().expect("down lock").push(id.to_string());
    }

    pub fn add_node(&self, id: &str) {
        let self_id = id.to_string();
        self.nodes
            .lock()
            .expect("nodes lock")
            .insert(self_id.clone(), None);
    }

    pub fn create_mutex(
        &self,
        id: &str,
        with_ids: Vec<String>,
    ) -> Arc<ModifiedLamportDistributedMutex> {
        let self_id = id.to_string();
        let net_for_req = self.nodes.clone();
        let net_for_rep = self.nodes.clone();
        let net_for_rel = self.nodes.clone();
        let down_ref = self.down.clone();
        let down_ref2 = self.down.clone();
        let down_ref3 = self.down.clone();

        let self_id_for_req = self_id.clone();
        let send_request = move |dest_id: &str, req: &MutexRequest| {
            {
                let down = down_ref.lock().expect("down");
                if down.iter().any(|x| x == dest_id) {
                    return;
                }
            }
            let dest_opt = {
                let map = net_for_req.lock().expect("net");
                map.get(dest_id)
                    .expect("Must have map entry in create_mutex")
                    .clone()
            };
            if let Some(dest) = dest_opt {
                dest.handle_request(&self_id_for_req, req);
            } else {
                tracing::warn!("REQUEST: No node with id {} in network", dest_id);
            }
        };

        let self_id_for_rep = self_id.clone();
        let send_reply = move |dest_id: &str, resp: &MutexResponse| {
            {
                let down = down_ref2.lock().expect("down");
                if down.iter().any(|x| x == dest_id) {
                    return;
                }
            }
            let dest_opt = {
                let map = net_for_rep.lock().expect("net");
                map.get(dest_id)
                    .expect("Must have map entry in create_mutex")
                    .clone()
            };
            if let Some(dest) = dest_opt {
                dest.handle_reply(&self_id_for_rep, resp);
            } else {
                tracing::warn!("REPLY: No node with id {} in network", dest_id);
            }
        };

        let self_id_for_rel = self_id.clone();
        let send_release = move |dest_id: &str, rel: &MutexRelease| {
            {
                let down = down_ref3.lock().expect("down");
                if down.iter().any(|x| x == dest_id) {
                    return;
                }
            }
            let dest_opt = {
                let map = net_for_rel.lock().expect("net");
                map.get(dest_id)
                    .expect("Must have map entry in create_mutex")
                    .clone()
            };
            if let Some(dest) = dest_opt {
                dest.handle_release(&self_id_for_rel, rel);
            } else {
                tracing::warn!("RELEASE: No node with id {} in network", dest_id);
            }
        };

        let mutex = Arc::new(ModifiedLamportDistributedMutex::new(
            id.to_string(),
            with_ids,
            send_request,
            send_reply,
            send_release,
        ));
        self.nodes
            .lock()
            .expect("nodes lock")
            .insert(self_id, Some(mutex.clone()));
        mutex
    }
}
