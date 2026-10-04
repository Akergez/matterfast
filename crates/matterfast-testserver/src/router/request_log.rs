use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Request logging: without it there is no way to tell "the client never
/// asked" from "the server answered wrongly", which is most of debugging.
pub(super) async fn request_log(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().to_string();
    let response = next.run(req).await;
    println!("{method} {path} -> {}", response.status().as_u16());
    response
}
