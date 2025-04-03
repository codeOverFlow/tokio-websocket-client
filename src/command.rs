#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Command {
    Reconnect,
    Close,
}
