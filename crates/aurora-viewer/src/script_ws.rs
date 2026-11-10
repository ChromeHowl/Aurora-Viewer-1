//! Script editor link: a WebSocket JSON-RPC server through which an
//! external editor (the Second Life extension of VS Code) follows the
//! scripts opened in it from the viewer and gets their compile results.
//! Port of the Legacy Viewer's script editor server (LLScriptEditorWSServer,
//! indra/newview/llscripteditorws.cpp; settings ExternalWebsocketSyncEnable,
//! ExternalWebsocketSyncPort, originally LGPL 2.1).
//!
//! - `wire`: the WebSocket upgrade and frames;
//! - `protocol`: the JSON-RPC session and methods, isolated, so a change
//!   of the protocol touches it alone;
//! - `server`: the sockets and threads.
//!
//! Differences, both stricter: off by default (on in the Legacy Viewer),
//! and always bound to 127.0.0.1 (ExternalWebsocketSyncLocal has no
//! counterpart).

pub mod protocol;
mod server;
mod wire;

pub use protocol::{Edited, Viewer};

/// The link as the preferences show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Off,
    Listening(u16),
    /// The port could not be taken (in use, refused): the reason.
    Failed(u16, String),
}

/// Starts and stops the server as the preferences say.
#[derive(Default)]
pub struct Link {
    server: Option<server::Server>,
    /// The port that could not be taken: not tried again each frame.
    failed: Option<(u16, String)>,
}

impl Link {
    pub fn status(&self) -> Status {
        match (&self.server, &self.failed) {
            (Some(s), _) => Status::Listening(s.port),
            (None, Some((port, reason))) => Status::Failed(*port, reason.clone()),
            (None, None) => Status::Off,
        }
    }

    /// Follows the preferences: on and on this port, or off.
    pub fn sync(&mut self, enabled: bool, port: u16) {
        if !enabled || port == 0 {
            self.server = None;
            self.failed = None;
            return;
        }
        if self.server.as_ref().is_some_and(|s| s.port == port) || self.failed.as_ref().is_some_and(|f| f.0 == port) {
            return;
        }
        self.server = None;
        match server::Server::start(port) {
            Ok(server) => {
                self.server = Some(server);
                self.failed = None;
            }
            Err(e) => {
                log::warn!("script editor link: port {port} not available: {e}");
                self.failed = Some((port, e.to_string()));
            }
        }
    }

    /// One frame of the link, while it runs.
    pub fn poll(&mut self, viewer: impl FnOnce() -> Viewer, edited: &[Edited]) {
        if let Some(server) = self.server.as_mut() {
            server.poll(&viewer(), edited, std::time::Instant::now());
        }
    }

    /// A script's save came back.
    pub fn compiled(&mut self, object: uuid::Uuid, item: uuid::Uuid, success: bool, running: bool, errors: &[String]) {
        if let Some(server) = self.server.as_mut() {
            server.compiled(&protocol::script_id(object, item), success, running, errors);
        }
    }
}
