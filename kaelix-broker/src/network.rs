//! Network handling for client connections.

/// Network server for handling client connections.
#[derive(Debug)]
pub struct NetworkServer {
    // TODO: Implement network server
}

impl NetworkServer {
    /// Create a new network server.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for NetworkServer {
    fn default() -> Self {
        Self::new()
    }
}
