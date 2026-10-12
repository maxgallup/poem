use poem::{Request, Response};

use crate::client::{ClientError, ClientTransport};

/// A transport that sends requests over the network with [`reqwest`].
///
/// TLS support depends on the features enabled for `reqwest`, add `reqwest`
/// with a TLS feature to your own dependencies to call `https` urls.
#[derive(Debug, Clone, Default)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Create a transport with a default [`reqwest::Client`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a transport that uses the specified [`reqwest::Client`].
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl From<reqwest::Client> for ReqwestTransport {
    fn from(client: reqwest::Client) -> Self {
        Self::with_client(client)
    }
}

impl ClientTransport for ReqwestTransport {
    async fn send(&self, req: Request) -> Result<Response, ClientError> {
        let (parts, body) = req.into_parts();
        if parts.uri.scheme().is_none() || parts.uri.authority().is_none() {
            return Err(ClientError::InvalidUrl {
                url: parts.uri.to_string(),
                reason: "the reqwest transport requires an absolute url".to_string(),
            });
        }

        let body = body.into_bytes().await.map_err(ClientError::transport)?;
        let resp = self
            .client
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(body)
            .send()
            .await
            .map_err(ClientError::transport)?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.bytes().await.map_err(ClientError::transport)?;

        let mut resp = Response::builder().status(status).body(body);
        *resp.headers_mut() = headers;
        Ok(resp)
    }
}
