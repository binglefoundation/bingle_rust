use crate::api::bingle_api::NetworkEndpoint;
use crate::dtls::Dtls;
use std::sync::{Arc, Mutex};

pub type PacketTransportHandleMessage = Arc<
    dyn Fn(&NetworkEndpoint, &str, &[u8]) -> Result<Option<Vec<u8>>, String> + Send + Sync,
>;

pub trait PacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String>;

    fn get_handle_message(&self) -> Option<PacketTransportHandleMessage>;

    fn set_handle_message(&mut self, handler: Option<PacketTransportHandleMessage>);

    fn with_handle_message(self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized;
}

pub struct DtlsReliablePacketTransport {
    dtls: Box<dyn Dtls + Send + Sync>,
    handle_message: Arc<Mutex<Option<PacketTransportHandleMessage>>>,
}

impl DtlsReliablePacketTransport {
    pub fn new(dtls: Box<dyn Dtls + Send + Sync>) -> Self {
        let mut transport = Self {
            dtls,
            handle_message: Arc::new(Mutex::new(None)),
        };

        let handle_message_for_dtls = transport.handle_message.clone();
        transport
            .dtls
            .set_handle_message(Some(Arc::new(move |_server, from, issuer, packet| {
                let handler = match handle_message_for_dtls.lock() {
                    Ok(g) => g.clone(),
                    Err(e) => {
                        tracing::warn!(
                            "[DtlsReliablePacketTransport::new] failed to lock handle_message: {}",
                            e
                        );
                        return;
                    }
                };

                if let Some(handler) = handler {
                    if let Err(e) = handler(from, issuer, packet) {
                        tracing::warn!(
                            "[DtlsReliablePacketTransport::new] packet transport handle_message failed: {}",
                            e
                        );
                    }
                } else {
                    tracing::warn!(
                        "[DtlsReliablePacketTransport::new] dtls callback received message but no packet transport handler configured"
                    );
                }
            })));

        transport
    }

    pub fn dtls(&self) -> &(dyn Dtls + Send + Sync) {
        self.dtls.as_ref()
    }

    pub fn dtls_mut(&mut self) -> &mut (dyn Dtls + Send + Sync) {
        self.dtls.as_mut()
    }

    pub fn dispatch_handle_message(
        &self,
        from: &NetworkEndpoint,
        issuer: &str,
        packet: &[u8],
    ) -> Result<Option<Vec<u8>>, String> {
        if let Some(handler) = self.get_handle_message() {
            handler(from, issuer, packet)
        } else {
            tracing::warn!(
                "[DtlsReliablePacketTransport::dispatch_handle_message] received message but no packet transport handler configured"
            );
            Ok(None)
        }
    }
}

impl PacketTransport for DtlsReliablePacketTransport {
    fn send(&self, to: &NetworkEndpoint, block: &[u8]) -> Result<(), String> {
        self.dtls.send(to, block)
    }

    fn get_handle_message(&self) -> Option<PacketTransportHandleMessage> {
        match self.handle_message.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::get_handle_message] failed to lock handler: {}",
                    e
                );
                None
            }
        }
    }

    fn set_handle_message(&mut self, handler: Option<PacketTransportHandleMessage>) {
        match self.handle_message.lock() {
            Ok(mut g) => {
                *g = handler;
            }
            Err(e) => {
                tracing::warn!(
                    "[DtlsReliablePacketTransport::set_handle_message] failed to lock handler: {}",
                    e
                );
            }
        }
    }

    fn with_handle_message(mut self, handler: PacketTransportHandleMessage) -> Self
    where
        Self: Sized,
    {
        self.set_handle_message(Some(handler));
        self
    }
}