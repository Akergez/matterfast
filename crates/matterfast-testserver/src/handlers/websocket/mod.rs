//! `GET /websocket`: the event stream the client listens on.

mod handle_socket;
mod upgrade;

pub(crate) use upgrade::websocket;
