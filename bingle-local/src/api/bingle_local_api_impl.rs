use crate::api::{BingleLocalApi, Contact, ContactSource, Keypair, Message};
use rust_comms::blockchain::algo_ops::AlgoOps;
use std::sync::Mutex;

/// Basic local implementation stub. For now it only supports keypair generation.
pub struct BingleApiLocalImpl {
    keypair: Mutex<Option<Keypair>>, // interior mutability to allow &self methods to ensure keypair exists
    algo_ops: Mutex<Option<AlgoOps>>, // cache constructed AlgoOps for current keypair
}

impl BingleApiLocalImpl {
    pub fn new() -> Self { Self { keypair: Mutex::new(None), algo_ops: Mutex::new(None) } }
}

impl BingleLocalApi for BingleApiLocalImpl {
    fn generate_keypair(&mut self) -> Result<Keypair, String> {
        let (id, passphrase) = rust_comms::blockchain::algo_ops::AlgoOps::generate_keypair();
        let kp = Keypair { id, passphrase };
        if let Ok(mut guard) = self.keypair.lock() {
            *guard = Some(kp.clone());
        }
        // Invalidate cached AlgoOps since keypair changed
        if let Ok(mut ops_guard) = self.algo_ops.lock() {
            *ops_guard = None;
        }
        Ok(kp)
    }

    fn register_keypair(&self) -> Result<bool, String> { Err("not implemented".to_string()) }

    fn get_algo_ops(&self) -> Result<rust_comms::blockchain::algo_ops::AlgoOps, String> {
        // 1) Return cached instance if available
        {
            let guard = self.algo_ops.lock().map_err(|_| "mutex poisoned".to_string())?;
            if let Some(ops) = guard.as_ref() {
                return Ok(ops.clone());
            }
        }

        // 2) No cached instance; require an existing keypair (do NOT generate here)
        let pass = {
            let guard = self.keypair.lock().map_err(|_| "mutex poisoned".to_string())?;
            guard
                .as_ref()
                .map(|k| k.passphrase.clone())
                .ok_or_else(|| "no keypair".to_string())?
        };

        // 3) Construct and cache AlgoOps bound to this passphrase
        let ops = AlgoOps::new(Some(pass), None, None);
        let mut cache_guard = self.algo_ops.lock().map_err(|_| "mutex poisoned".to_string())?;
        *cache_guard = Some(ops.clone());
        Ok(ops)
    }

    fn add_contact(&mut self, _handle: String, _id: String, _source: ContactSource) -> Result<(), String> {
        Err("not implemented".to_string())
    }

    fn block_contact(&mut self, _id: String) -> Result<(), String> { Err("not implemented".to_string()) }

    fn remove_contact(&mut self, _id: String) -> Result<(), String> { Err("not implemented".to_string()) }

    fn is_blocked(&self, _id: &str) -> Result<bool, String> { Err("not implemented".to_string()) }

    fn get_contacts(&self) -> Result<Vec<Contact>, String> { Err("not implemented".to_string()) }

    fn add_message(
        &mut self,
        _sender_handle: String,
        _recipient_handles: Vec<String>,
        _timestamp: i64,
        _text: String,
    ) -> Result<(), String> {
        Err("not implemented".to_string())
    }

    fn get_messages(&self) -> Result<Vec<Message>, String> { Err("not implemented".to_string()) }
}
