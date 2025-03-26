use std::pin::Pin;
use crate::message::CloseCode;

pub enum RetryStrategy {
    Retry,
    Close,
}

pub trait Handler: Send {
    type Error: std::error::Error + Send;
    
    fn on_text(&mut self, text: &str) -> impl Future<Output=Result<(), Self::Error>> + Send;
    fn on_binary(&mut self, buffer: &[u8]) -> impl Future<Output=Result<(), Self::Error>> + Send;
    fn on_connect(&mut self) -> impl Future<Output=()> + Send;
    fn on_connect_failure(&mut self) -> impl Future<Output=Result<RetryStrategy, Self::Error>> + Send;
    fn on_close(&mut self, code: CloseCode, reason: &str) -> impl Future<Output=Result<RetryStrategy, Self::Error>> + Send;
}