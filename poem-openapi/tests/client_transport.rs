#![cfg(feature = "client")]

use poem::{
    Error, Request, Route, handler,
    http::{Method, StatusCode},
    web::Path,
};
use poem_openapi::client::{Client, ClientError, ClientTransport};

#[handler]
fn echo(Path(name): Path<String>, req: &Request, body: String) -> String {
    format!(
        "{} {} {} {}",
        req.method(),
        name,
        req.header("x-test").unwrap_or_default(),
        body
    )
}

#[handler]
fn conflict() -> poem::Result<()> {
    Err(Error::from_status(StatusCode::CONFLICT))
}

fn app() -> Route {
    Route::new().nest(
        "/api",
        Route::new()
            .at("/hello/:name", poem::post(echo))
            .at("/conflict", conflict),
    )
}

async fn check_transport<Tr: ClientTransport>(client: Client<Tr>) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(client.url("/hello/world").unwrap())
        .header("x-test", "abc")
        .body("the body");
    let resp = client.send(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.into_body().into_string().await.unwrap(),
        "POST world abc the body"
    );

    // errors returned by the server are received as responses
    let req = Request::builder()
        .uri(client.url("/conflict").unwrap())
        .finish();
    let resp = client.send(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let req = Request::builder()
        .uri(client.url("/not-found").unwrap())
        .finish();
    let resp = client.send(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn endpoint_transport() {
    check_transport(Client::new("/api", app()).unwrap()).await;
}

#[tokio::test]
async fn invalid_base_url() {
    assert!(matches!(
        Client::new("api", app()),
        Err(ClientError::InvalidUrl { .. })
    ));
}

#[cfg(feature = "client-reqwest")]
mod reqwest_transport {
    use poem::{
        Server,
        listener::{Acceptor, Listener, TcpListener},
    };
    use poem_openapi::client::ReqwestTransport;

    use super::*;

    #[tokio::test]
    async fn reqwest_transport() {
        let acceptor = TcpListener::bind("127.0.0.1:0")
            .into_acceptor()
            .await
            .unwrap();
        let addr = *acceptor.local_addr()[0].as_socket_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(
            Server::new_with_acceptor(acceptor).run_with_graceful_shutdown(
                app(),
                async move {
                    let _ = rx.await;
                },
                None,
            ),
        );

        let client = Client::new(format!("http://{addr}/api"), ReqwestTransport::new()).unwrap();
        check_transport(client).await;

        tx.send(()).unwrap();
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn reqwest_requires_absolute_url() {
        let client = Client::new("/api", ReqwestTransport::new()).unwrap();
        let req = Request::builder()
            .uri(client.url("/conflict").unwrap())
            .finish();
        assert!(matches!(
            client.send(req).await,
            Err(ClientError::InvalidUrl { .. })
        ));
    }
}
