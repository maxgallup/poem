use std::future::Future;

use poem::{Endpoint, Request, Response};

use crate::client::ClientError;

/// Represents a transport that sends requests for an OpenAPI client.
///
/// # Provided Implementations
///
/// - **E: poem::Endpoint**
///
///  Calls the endpoint in-process, without any network. Errors returned by the
///  endpoint are converted to responses, exactly as the server would do.
///
/// - **ReqwestTransport**
///
///  Sends the request over the network with [`reqwest`]. Requires the
///  `client-reqwest` feature.
pub trait ClientTransport: Send + Sync {
    /// Sends the request and returns the response.
    fn send(&self, req: Request) -> impl Future<Output = Result<Response, ClientError>> + Send;
}

impl<E: Endpoint> ClientTransport for E {
    async fn send(&self, req: Request) -> Result<Response, ClientError> {
        Ok(self.get_response(req).await)
    }
}
