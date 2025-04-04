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
    /// Handle text messages.
    ///
    /// Any error will trigger a reconnection.
    fn on_text(&mut self, text: &str) -> impl Future<Output = ()> + Send;

    /// Handle binary messages.
    ///
    /// Any error will trigger a reconnection.
    fn on_binary(&mut self, buffer: &[u8]) -> impl Future<Output = ()> + Send;

    /// Defines the behavior for when the server closes the connection.
    ///
    /// Return the [`RetryStrategy`] to use.
    #[inline]
    #[expect(
        unused_variables,
        reason = "default implementation does not use parameters but prefixing with `_` would show in the documentation"
    )]
    fn on_close(
        &mut self,
        code: CloseCode,
        reason: &str,
    ) -> impl Future<Output = RetryStrategy> + Send {
        async move { RetryStrategy::Retry }
    }

    /// Defines a callback for when connection succeeds.
    #[inline]
    fn on_connect(&mut self) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// Defines the behavior for when the connection fails.
    ///
    /// Return the [`RetryStrategy`] to use.
    #[inline]
    fn on_connect_failure(&mut self) -> impl Future<Output = RetryStrategy> + Send {
        async move { RetryStrategy::Retry }
    }

    /// Defines a callback for when connection is broken.
    #[inline]
    fn on_disconnect(&mut self) -> impl Future<Output = RetryStrategy> + Send {
        async move { RetryStrategy::Retry }
    }
}
