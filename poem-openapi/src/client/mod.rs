//! Client support for OpenAPI services.

mod error;
#[cfg(feature = "client-reqwest")]
mod reqwest_transport;
mod transport;

use poem::{Request, Response, http::Uri};

#[cfg(feature = "client-reqwest")]
pub use self::reqwest_transport::ReqwestTransport;
pub use self::{error::ClientError, transport::ClientTransport};

/// The core of an OpenAPI client, it combines a base url with a transport.
///
/// The base url is either absolute (`http://localhost:3000/api`), or only a
/// path (`/api`) when the transport calls an endpoint in-process.
#[derive(Debug, Clone)]
pub struct Client<Tr> {
    base_url: String,
    transport: Tr,
}

impl<Tr: ClientTransport> Client<Tr> {
    /// Create a client with the specified base url and transport.
    pub fn new(base_url: impl Into<String>, transport: Tr) -> Result<Self, ClientError> {
        let base_url = base_url.into();
        let invalid = |reason: &str| ClientError::InvalidUrl {
            url: base_url.clone(),
            reason: reason.to_string(),
        };

        if !base_url.is_empty() {
            let uri = base_url
                .parse::<Uri>()
                .map_err(|err| invalid(&err.to_string()))?;
            if uri.query().is_some() {
                return Err(invalid("the base url must not contain a query string"));
            }
            if uri.scheme().is_none() && !base_url.starts_with('/') {
                return Err(invalid("the base url must be absolute or start with a `/`"));
            }
        }

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            transport,
        })
    }

    /// Returns the base url, without a trailing `/`.
    #[inline]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the transport.
    #[inline]
    pub fn transport(&self) -> &Tr {
        &self.transport
    }

    /// Returns the url of the specified path, relative to the base url.
    pub fn url(&self, path: &str) -> Result<Uri, ClientError> {
        let url = if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        };
        url.parse::<Uri>().map_err(|err| ClientError::InvalidUrl {
            url,
            reason: err.to_string(),
        })
    }

    /// Sends the request with the transport.
    pub async fn send(&self, req: Request) -> Result<Response, ClientError> {
        self.transport.send(req).await
    }
}

#[cfg(test)]
mod tests {
    use poem::{Route, handler};

    use super::*;

    #[handler]
    fn index() {}

    fn client(base_url: &str) -> Result<Client<Route>, ClientError> {
        Client::new(base_url, Route::new().at("/", index))
    }

    #[test]
    fn base_url() {
        assert_eq!(client("").unwrap().base_url(), "");
        assert_eq!(client("/").unwrap().base_url(), "");
        assert_eq!(client("/api/").unwrap().base_url(), "/api");
        assert_eq!(
            client("http://localhost:3000/api/").unwrap().base_url(),
            "http://localhost:3000/api"
        );

        assert!(matches!(client("api"), Err(ClientError::InvalidUrl { .. })));
        assert!(matches!(
            client("http://localhost/api?a=1"),
            Err(ClientError::InvalidUrl { .. })
        ));
        assert!(matches!(
            client("http://local host"),
            Err(ClientError::InvalidUrl { .. })
        ));
    }

    #[test]
    fn url() {
        let c = client("").unwrap();
        assert_eq!(c.url("/users/1").unwrap(), "/users/1");
        assert_eq!(c.url("users/1").unwrap(), "/users/1");

        let c = client("http://localhost:3000/api/").unwrap();
        assert_eq!(
            c.url("/users/1").unwrap(),
            "http://localhost:3000/api/users/1"
        );
        assert_eq!(c.url("users").unwrap(), "http://localhost:3000/api/users");
    }
}
