//! Templum's HTTP API (docs/specs/api.md): tokens, TLS, the server, the registry and the event
//! stream. No UI: `crates/templum` holds the glue.

mod audit;
mod events;
pub mod model;
pub mod openapi;
pub mod registry;
pub mod server;
pub mod store;
mod terminals;
pub mod tls;

pub use model::{Caller, Notice, State};
pub use server::{Client, Options, Server, start};
pub use store::{Scope, Store, Token};
pub use terminals::TerminalRef;

/// The asker of the API's tasks: a terminal id is `<pid>-<n>`, never `api`.
pub const ASKER: &str = "api";
/// A bus `Hello.client` the API's messages carry. The bus refuses it on its socket.
pub const CLIENT: &str = "api";
/// The method of the message that asks the UI thread for the state.
pub const STATE: &str = "templum/state";
/// The method of the notification that says a stream opened: the UI thread starts its tick.
pub const WATCH: &str = "templum/watch";
/// The method of the request a terminal stream's opening sends the UI thread, params
/// `{"block": …}`: the glue checks the block for the token, lends every terminal and answers as a
/// tool does (ok, `isError` with its sentence, or an error code).
pub const TERMINAL: &str = "templum/terminal";
/// The method of the notification that says a client connected or left: the pill counts them.
pub const CLIENTS: &str = "templum/clients";

/// JSON-RPC errors the glue answers, mapped to 403, 401 and 404.
pub mod codes {
    pub const SCOPE: i32 = -32001;
    pub const AUTH: i32 = -32003;
    pub const NOT_FOUND: i32 = -32004;
}
