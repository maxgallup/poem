use bytes::Bytes;
use poem::http::{HeaderMap, StatusCode};
use thiserror::Error;

/// An error returned by an OpenAPI client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClientError {
    /// The base url or a request url is invalid.
    #[error("invalid url `{url}`: {reason}")]
    InvalidUrl {
        /// The invalid url.
        url: String,

        /// The reason for the error.
        reason: String,
    },

    /// The transport failed to send the request or receive the response.
    #[error("transport error: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// The server responded with a status code that is not declared for the
    /// operation.
    #[error("unexpected status code: {status}")]
    UnexpectedStatus {
        /// The status code of the response.
        status: StatusCode,

        /// The headers of the response.
        headers: Box<HeaderMap>,

        /// The body of the response.
        body: Bytes,
    },

    /// The response could not be decoded.
    #[error("failed to decode response: {0}")]
    Decode(String),
}

impl ClientError {
    /// Creates a transport error from any error type.
    pub fn transport(err: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Self::Transport(err.into())
    }
}
