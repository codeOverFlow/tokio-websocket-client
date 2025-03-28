use crate::message::CloseCode;

/// Retry strategy to use when connection fails or server close the connection.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RetryStrategy {
    /// Retry connecting to the server.
    Retry,
    /// Close the connection and end all tasks.
    Close,
}

/// Defines how to handle text, binary and close messages from the server.
///
/// It also allows to define [`RetryStrategy`] when connection fails and a callback on connection.
pub trait Handler: Send {
    /// The error type for every callback.
    type Error: std::error::Error + Send;

    /// Handle text messages.
    ///
    /// Any error will trigger a reconnection.
    fn on_text(&mut self, text: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Handle binary messages.
    ///
    /// Any error will trigger a reconnection.
    fn on_binary(&mut self, buffer: &[u8]) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Defines the behavior for when the server closes the connection.
    ///
    /// Return the [`RetryStrategy`] to use.
    #[allow(unused_variables)]
    fn on_close(
        &mut self,
        code: CloseCode,
        reason: &str,
    ) -> impl Future<Output = RetryStrategy> + Send {
        async move { RetryStrategy::Retry }
    }

    /// Defines a callback for when connection succeeds.
    fn on_connect(&mut self) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// Defines the behavior for when the connection fails.
    ///
    /// Return the [`RetryStrategy`] to use.
    fn on_connect_failure(&mut self) -> impl Future<Output = RetryStrategy> + Send {
        async move { RetryStrategy::Retry }
    }
}
