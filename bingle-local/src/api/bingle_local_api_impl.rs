use crate::api::{BingleLocalApi, Contact, ContactSource, Keypair, Message};

/// Basic local implementation stub. For now it only supports keypair generation.
#[derive(Default)]
pub struct BingleApiLocalImpl {
    keypair: Option<Keypair>,
}

impl BingleApiLocalImpl {
    pub fn new() -> Self { Self { keypair: None } }
}

impl BingleLocalApi for BingleApiLocalImpl {
    fn generate_keypair(&mut self) -> Result<Keypair, String> {
        let (id, passphrase) = rust_comms::blockchain::algo_ops::AlgoOps::generate_keypair();
        let kp = Keypair { id, passphrase };
        self.keypair = Some(kp.clone());
        Ok(kp)
    }

    fn register_keypair(&self) -> Result<bool, String> { Err("not implemented".to_string()) }

    fn get_algo_ops(&self) -> Result<rust_comms::blockchain::algo_ops::AlgoOps, String> {
        Err("not implemented".to_string())
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
