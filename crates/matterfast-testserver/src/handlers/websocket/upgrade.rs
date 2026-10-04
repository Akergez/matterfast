use std::sync::Arc;

use axum::extract::ws::WebSocketUpgrade;
use axum::extract::State;
use axum::response::Response;

use super::handle_socket::handle_socket;
use crate::app::App;

pub(crate) async fn websocket(ws: WebSocketUpgrade, State(app): State<Arc<App>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, app))
}
