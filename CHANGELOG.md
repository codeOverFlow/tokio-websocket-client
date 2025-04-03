# 0.2.0
## Removed
* `Handler::Error`: it has been removed due to changes on the trait.
* `Client::text_timeout`: it has been removed because tokio already provides a `timeout` feature.
* `Client::binary_timeout`: it has been removed because tokio already provides a `timeout` feature.

## Changed
* `Handler::on_text` no longer return a `Result`.
* `Handler::on_binary` no longer return a `Result`.
*

## Added
* `Handler::on_disconnect`: allow to give a `RetryStrategy` when the connection is lost.
* `Client::blocking_text`: allow to send text messages in a synchronous context.
* `Client::blocking_text_timeout`: allow to send text messages with a timeout in a synchronous context.
* `Client::blocking_binary`: allow to send binary messages in a synchronous context.
* `Client::blocking_binary_timeout`: allow to send binary messages with a timeout in a synchronous context.
* `Client::force_reconnect`: allow to force the reconnection.
* `Client::blocking_force_reconnect`: allow to force the reconnection in a synchronous context.
* `Client::blocking_force_reconnect_timeout`: allow to force the reconnection with a timeout in a synchronous context.
* `Client::close`: allow to close the connection.
* `Client::blocking_close`: allow to close the connection in a synchronous context.
* `Client::blocking_close_timeout`: allow to close the connection with a timeout in a synchronous context.

