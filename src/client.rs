use crate::{Message};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) to_send: flume::Sender<Message>,
}

impl From<Client> for flume::Sender<Message> {
    fn from(client: Client) -> flume::Sender<Message> {
        client.to_send
    }
}

impl Client {
    pub async fn text(&self, message: impl Into<String>) -> Result<(), flume::SendError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {}", &message);
        
        self.to_send.send_async(Message::Text(message)).await
    }

    pub async fn text_timeout(&self, message: impl Into<String>, timeout: Duration) -> Result<(), std::io::Error> {
        let message = message.into();
        log::debug!("Sending text: {}", &message);
        match tokio::time::timeout(timeout, self.to_send.send_async(Message::Text(message))).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Receiver channel has been dropped.")),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Time out reached.")),
        }
    }

    pub async fn binary(&self, message: impl IntoIterator<Item=u8>) -> Result<(), flume::SendError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);

        self.to_send.send_async(Message::Binary(message)).await
    }

    pub async fn binary_timeout(&self, message: impl IntoIterator<Item=u8>, timeout: Duration) -> Result<(), std::io::Error> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);
        match tokio::time::timeout(timeout, self.to_send.send_async(Message::Binary(message))).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "Receiver channel has been dropped.")),
            Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Time out reached.")),
        }
    }
}