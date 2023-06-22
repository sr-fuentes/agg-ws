use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unexpected Shutdown")]
    UnexpectedShutdown,
    #[error("Socket Does Not Exist")]
    SocketDoesNotExist,
    #[error("Response Does Not Match Channel")]
    ChannelResponseMismatch,
    #[error("Channel Does Not Exist")]
    ChannelDoesNotExist,
    #[error("Channel Already Subscribed")]
    ChannelAlreadySubscribed,
    #[error(transparent)]
    Oneshot(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
