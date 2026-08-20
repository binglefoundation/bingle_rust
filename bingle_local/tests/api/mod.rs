use std::collections::{HashMap, HashSet};

use bingle_local::api::{BingleLocalApi, Contact, ContactSource, Keypair, KeypairStatus, Message};
use algo_ops::AlgoOps;

#[derive(Default)]
struct DummyLocal {
    keypair: Option<Keypair>,
    contacts: Vec<Contact>,
    blocked: HashSet<String>,
    messages: Vec<Message>,
}

impl BingleLocalApi for DummyLocal {
    fn generate_keypair(&mut self) -> Result<Keypair, String> {
        let kp = Keypair { id: "TEST_ID".into(), passphrase: "TEST_PASSPHRASE".into() };
        self.keypair = Some(kp.clone());
        Ok(kp)
    }

    fn register_keypair(&self, _handle: String) -> Result<(), String> { Ok(()) }

    fn get_algo_ops(&self) -> Result<AlgoOps, String> {
        let pass = self
            .keypair
            .as_ref()
            .map(|k| k.passphrase.clone())
            .ok_or_else(|| "no keypair".to_string())?;
        Ok(AlgoOps::new(Some(pass), None, None))
    }

    fn add_contact(&mut self, handle: String, id: String, _source: ContactSource) -> Result<(), String> {
        let c = Contact { handle, id, fields: HashMap::new() };
        self.contacts.push(c);
        Ok(())
    }

    fn block_contact(&mut self, id: String) -> Result<(), String> {
        self.blocked.insert(id);
        Ok(())
    }

    fn remove_contact(&mut self, id: String) -> Result<(), String> {
        self.contacts.retain(|c| c.id != id);
        Ok(())
    }

    fn is_blocked(&self, id: &str) -> Result<bool, String> { Ok(self.blocked.contains(id)) }

    fn get_contacts(&self) -> Result<Vec<Contact>, String> {
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
    ) -> Result<(), String> {
        self.messages.push(Message { sender_handle, recipient_handles, timestamp, text, cipher_suite });
        Ok(())
    }

    fn get_messages(&self) -> Result<Vec<Message>, String> { Ok(self.messages.clone()) }

    fn save(&self, _path: &str) -> Result<(), String> { Ok(()) }

    fn load(&mut self, _path: &str) -> Result<(), String> { Ok(()) }

    fn keypair_status(&self) -> Result<KeypairStatus, String> {
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

    fn get_keypair(&self) -> Result<Option<Keypair>, String> {
        Ok(self.keypair.clone())
    }
}

#[test]
fn test_bingle_local_api_smoke() {
    let mut api = DummyLocal::default();

    // generate keypair
    let kp = api.generate_keypair().expect("keypair");
    assert_eq!(kp.id, "TEST_ID");

    // contacts
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual).unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received).unwrap();
    api.block_contact("ID_BOB".into()).unwrap();

    let contacts = api.get_contacts().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "alice");
    assert!(!api.is_blocked("ID_ALICE").unwrap());
    assert!(api.is_blocked("ID_BOB").unwrap());

    // messages
    api.add_message("alice".into(), vec!["bob".into()], 1_725_000_000_000, "hi".into(), None).unwrap();
    let msgs = api.get_messages().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "hi");

    // algo ops
    let ops = api.get_algo_ops().expect("ops");
    // We don't assume address presence; just ensure the struct exists
    let _ = ops; // suppress unused warning
}
