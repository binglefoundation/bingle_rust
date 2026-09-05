use std::collections::{HashMap, HashSet};

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_local::api::{BingleLocalApi, Contact, ContactSource, Keypair, KeypairStatus, Message};

#[derive(Default)]
struct DummyLocal {
    keypair: Option<Keypair>,
    contacts: Vec<Contact>,
    blocked: HashSet<String>,
    messages: Vec<Message>,
}

impl BingleLocalApi for DummyLocal {
    fn generate_keypair(&mut self) -> Result<Keypair, BingleError> {
        let kp = Keypair {
            id: "TEST_ID".into(),
            passphrase: "TEST_PASSPHRASE".into(),
        };
        self.keypair = Some(kp.clone());
        Ok(kp)
    }

    fn import_keypair(&mut self, passphrase: String) -> Result<Keypair, BingleError> {
        let kp = Keypair {
            id: "TEST_ID".into(),
            passphrase,
        };
        self.keypair = Some(kp.clone());
        Ok(kp)
    }

    fn register_keypair(&self, _handle: String) -> Result<bool, BingleError> {
        Ok(true)
    }

    fn register_apns_token(&self, _token: Vec<u8>) -> Result<bool, BingleError> {
        Ok(true)
    }

    fn ensure_local_migrated(&self) -> Result<Option<String>, BingleError> {
        Ok(None)
    }

    fn get_algo_ops(&self) -> Result<AlgoOps, BingleError> {
        let pass = self
            .keypair
            .as_ref()
            .map(|k| k.passphrase.clone())
            .ok_or_else(|| BingleError::Other("no keypair".to_string()))?;
        Ok(AlgoOps::new_for_algorand(Some(pass), None, None))
    }

    fn add_contact(
        &mut self,
        handle: String,
        id: String,
        _source: ContactSource,
    ) -> Result<(), BingleError> {
        let c = Contact {
            handle,
            id,
            fields: HashMap::new(),
        };
        self.contacts.push(c);
        Ok(())
    }

    fn block_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.blocked.insert(id);
        Ok(())
    }

    fn remove_contact(&mut self, id: String) -> Result<(), BingleError> {
        self.contacts.retain(|c| c.id != id);
        Ok(())
    }

    fn is_blocked(&self, id: &str) -> Result<bool, BingleError> {
        Ok(self.blocked.contains(id))
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError> {
        let v = self
            .contacts
            .iter()
            .filter(|c| !self.blocked.contains(&c.id))
            .cloned()
            .collect();
        Ok(v)
    }

    fn add_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
        cipher_suite: Option<String>,
    ) -> Result<(), BingleError> {
        self.messages.push(Message {
            sender_handle,
            recipient_handles,
            timestamp,
            text,
            cipher_suite,
            progress: Some(1.0),
            failure_reason: None,
            failure_kind: None,
            sent_time: None,
            delivered_time: None,
            signature: None,
        });
        Ok(())
    }

    fn queue_message(
        &mut self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleError> {
        let sender_handle = self.keypair_status()?.handle.unwrap_or_default();
        self.messages.push(Message {
            sender_handle,
            recipient_handles,
            timestamp: 999,
            text,
            cipher_suite: None,
            progress: Some(0.0),
            failure_reason: None,
            failure_kind: None,
            sent_time: None,
            delivered_time: None,
            signature: None,
        });
        Ok(())
    }

    fn update_message_status(
        &mut self,
        timestamp: i64,
        progress: f32,
        failure_reason: Option<String>,
        failure_kind: Option<SendFailureKind>,
    ) -> Result<(), BingleError> {
        if let Some(m) = self.messages.iter_mut().find(|m| m.timestamp == timestamp) {
            m.progress = Some(progress);
            m.failure_reason = failure_reason;
            m.failure_kind = failure_kind;
        }
        Ok(())
    }

    fn get_pending_messages(&self) -> Result<Vec<Message>, BingleError> {
        Ok(self
            .messages
            .iter()
            .filter(|m| m.progress.map_or(false, |p| p < 1.0))
            .cloned()
            .collect())
    }

    fn get_messages(&self) -> Result<Vec<Message>, BingleError> {
        Ok(self.messages.clone())
    }

    fn poll_mailbox(&self) -> Result<Vec<Message>, BingleError> {
        // This dummy does no store-and-forward reading.
        Ok(Vec::new())
    }

    fn save(&self, _path: &str) -> Result<(), BingleError> {
        Ok(())
    }

    fn load(&mut self, _path: &str) -> Result<(), BingleError> {
        Ok(())
    }

    fn network_available(&self, _force_recheck: bool) -> Result<bool, BingleError> {
        Ok(true)
    }

    fn keypair_status(&self) -> Result<KeypairStatus, BingleError> {
        match &self.keypair {
            Some(kp) => Ok(KeypairStatus {
                status: "ACTIVE".to_string(),
                id: Some(kp.id.clone()),
                handle: Some("test_handle".to_string()),
                required_algo: None,
                stale: false,
            }),
            None => Ok(KeypairStatus {
                status: "None".to_string(),
                id: None,
                handle: None,
                required_algo: None,
                stale: false,
            }),
        }
    }

    fn get_keypair(&self) -> Result<Option<Keypair>, BingleError> {
        Ok(self.keypair.clone())
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn test_bingle_local_api_smoke() {
    let mut api = DummyLocal::default();

    // generate keypair
    let kp = api.generate_keypair().expect("keypair");
    assert_eq!(kp.id, "TEST_ID");

    // contacts
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .unwrap();
    api.block_contact("ID_BOB".into()).unwrap();

    let contacts = api.get_contacts().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "alice");
    assert!(!api.is_blocked("ID_ALICE").unwrap());
    assert!(api.is_blocked("ID_BOB").unwrap());

    // messages
    api.add_message(
        "alice".into(),
        vec!["bob".into()],
        1_725_000_000_000,
        "hi".into(),
        None,
    )
    .unwrap();
    let msgs = api.get_messages().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "hi");

    // algo ops
    let ops = api.get_algo_ops().expect("ops");
    // We don't assume address presence; just ensure the struct exists
    let _ = ops; // suppress unused warning
}
