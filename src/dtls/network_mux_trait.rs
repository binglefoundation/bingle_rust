use std::net::{SocketAddr, ToSocketAddrs};
use std::any::Any;

/// Local Result type for NetworkMux operations with string error messages.
pub type Result<T = ()> = core::result::Result<T, String>;

/**
 * Handle DTLS packets arriving on the mux
 * @param source the NetworkMux implementation invoking the handler
 * @param from_address the address of the peer
 * @param data the datagram payload
 */
pub type HandleDtls = std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync + 'static>;

/**
 * Handle STUN packets arriving on the mux
 * @param source the NetworkMux implementation invoking the handler
 * @param from_address the address of the peer
 * @param data the datagram payload
 */
pub type HandleStun = std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync + 'static>;

/**
 * Handle TURN packets arriving on the mux
 * @param source the NetworkMux implementation invoking the handler
 * @param from_address the address of the peer
 * @param data the datagram payload
 */
pub type HandleTurn = std::sync::Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync + 'static>;

/// Public NetworkMux trait abstraction
pub trait NetworkMux {
    /**
     * Write a datagram to the underlying transport to the specified destination.
     * @param to the destination address (any ToSocketAddrs)
     * @param buf the datagram payload to write
     * @return Ok(()) if the data was queued/sent, Err(()) otherwise
     */
    fn write<A: ToSocketAddrs>(&self, to: A, buf: &[u8]) -> Result<()>
    where
        Self: Sized;

    /**
     * Get the DTLS handler function
     */
    fn get_handle_dtls(&self) -> Option<HandleDtls>;

    /**
     * Set the DTLS handler function
     */
    fn set_handle_dtls(&mut self, handler: Option<HandleDtls>);

    /**
     * Fluently set the DTLS handler function
     */
    fn with_handle_dtls(self, handler: HandleDtls) -> Self
    where
        Self: Sized;

    /**
     * Get the STUN handler function
     */
    fn get_handle_stun(&self) -> Option<HandleStun>;

    /**
     * Set the STUN handler function
     */
    fn set_handle_stun(&mut self, handler: Option<HandleStun>);

    /**
     * Fluently set the STUN handler function
     */
    fn with_handle_stun(self, handler: HandleStun) -> Self
    where
        Self: Sized;

    /**
     * Get the TURN handler function
     */
    fn get_handle_turn(&self) -> Option<HandleTurn>;

    /**
     * Set the TURN handler function
     */
    fn set_handle_turn(&mut self, handler: Option<HandleTurn>);

    /**
     * Fluently set the TURN handler function
     */
    fn with_handle_turn(self, handler: HandleTurn) -> Self
    where
        Self: Sized;

    // Downcast support to access concrete implementations when needed
    fn as_any(&self) -> &dyn Any;
}
