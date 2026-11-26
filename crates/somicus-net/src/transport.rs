//! Session transport interfaces and connection abstractions.

use std::future::Future;
use std::pin::Pin;

use crate::Redacted;
use crate::error::NetError;
use crate::proxy::ProxyConfig;

/// Type alias for boxed asynchronous transport operations.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A duplex session connection (WebSocket or `HTTP/2` stream). No transport opens one yet (B6.02).
pub trait SessionConnection: Send + Sync {
    /// Sends a pre-redacted binary frame across the egress connection.
    ///
    /// # Invariant
    /// The argument MUST be [`Redacted<Vec<u8>>`]. Unredacted buffers are rejected by type system.
    fn send(&mut self, payload: Redacted<Vec<u8>>) -> BoxFuture<'_, Result<(), NetError>>;

    /// Receives a binary frame from the gateway.
    fn receive(&mut self) -> BoxFuture<'_, Result<Option<Vec<u8>>, NetError>>;

    /// Closes the connection gracefully.
    fn close(&mut self) -> BoxFuture<'_, Result<(), NetError>>;
}

/// The core network transport interface for session establishment and egress.
///
/// # Security Invariant
/// Every egress method accepts ONLY [`Redacted<T>`] payloads.
/// Passing unredacted types is prevented at compile time.
pub trait Transport: Send + Sync {
    /// Returns the human-readable identifier of this transport implementation.
    fn name(&self) -> &'static str;

    /// Returns the active proxy configuration.
    fn proxy_config(&self) -> &ProxyConfig;

    /// Opens a duplex session connection to the Agent Gateway using a single-use session ticket and `DPoP` proof.
    fn connect<'a>(
        &'a self,
        endpoint: &'a str,
        session_ticket: &'a Redacted<String>,
        dpop_proof: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn SessionConnection>, NetError>>;

    /// Sends a pre-redacted `HTTP` POST request to a Somicus service.
    fn post_redacted<'a>(
        &'a self,
        url: &'a str,
        body: Redacted<Vec<u8>>,
        dpop_proof: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Vec<u8>, NetError>>;
}
