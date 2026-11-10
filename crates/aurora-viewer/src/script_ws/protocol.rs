//! The script editor link's JSON-RPC 2.0 protocol, version 1.0, as the
//! Legacy Viewer speaks it to external editors (the Second Life extension
//! of VS Code): LLScriptEditorWSServer and LLScriptEditorWSConnection
//! (indra/newview/llscripteditorws.cpp, originally LGPL 2.1), and the
//! extension's published message interfaces.
//!
//! The protocol lives here alone, as a state machine with no socket nor
//! file: messages in, messages out. A change of the protocol is a change of
//! this module.
//!
//! The session: on connection the viewer calls `session.handshake` with a
//! challenge, the path of a file only the user can read holding a secret;
//! the client answers with what the file says (`challenge_response`). A
//! wrong answer, an error, or none within 30 seconds closes the connection;
//! before the right one, every request is refused (-32002) and nothing else
//! is sent. Then `session.ok`, and the client may subscribe to the scripts
//! the viewer edits externally (`script.subscribe`, by their script id) to
//! hear their compile results (`script.compiled`).
//!
//! What Aurora serves of it: the session and system methods, the script
//! subscriptions and their compile results, and the syntax id. Not served
//! (method not found, and not announced in the handshake's features): the
//! syntax files, the object publishing (`object.*`) and the commands, and
//! Luau, which Aurora does not compile.

use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// The Legacy Viewer's ExternalWebsocketSyncPort default.
pub const DEFAULT_PORT: u16 = 9020;
/// HANDSHAKE_TIMEOUT: the time a client has to answer the challenge.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const PROTOCOL_VERSION: &str = "1.0";
const SERVER_VERSION: &str = "1.0.0";

/// Standard and server error codes (the published interfaces).
pub mod code {
    pub const PARSE: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const AUTHENTICATION_REQUIRED: i64 = -32002;
}

/// session.disconnect's reasons (DisconnectReason) the viewer gives; 0
/// (normal) and 1 (editor closed) are the client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disconnect {
    ProtocolError = 2,
    Timeout = 3,
    InternalError = 4,
}

/// script.subscribe's status (SubscriptionError). 1, the editor gone, is
/// never given: a closed editor's script is no longer offered at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subscribe {
    Success = 0,
    InvalidSubscription = 2,
    AlreadySubscribed = 3,
}

impl Subscribe {
    fn message(self) -> &'static str {
        match self {
            Subscribe::Success => "OK",
            Subscribe::InvalidSubscription => "No subscription found for script",
            Subscribe::AlreadySubscribed => "Script already subscribed",
        }
    }
}

/// The methods a client may call once let in.
const METHODS: [&str; 8] = [
    "system.ping",
    "system.getVersion",
    "system.status",
    "system.listMethods",
    "script.subscribe",
    "script.unsubscribe",
    "script.list",
    "language.syntax.id",
];

/// A script's id on the link (buildScriptSubscriptionId): the MD5 of
/// "<prim id>_<item id>" in hex. The same id names its file
/// (`sl_script_<name>_<id>.lsl`), which is how the extension knows which
/// script a file it opens is.
pub fn script_id(object: Uuid, item: Uuid) -> String {
    format!("{:x}", md5::compute(format!("{object}_{item}")))
}

/// The name of a script's file for an external editor
/// (LLScriptEdContainer::getTmpFileName), `name` already made safe.
pub fn file_name(name: &str, object: Uuid, item: Uuid) -> String {
    let id = script_id(object, item);
    if name.is_empty() {
        format!("sl_script_{id}.lsl")
    } else {
        format!("sl_script_{name}_{id}.lsl")
    }
}

/// The viewer as the handshake presents it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Viewer {
    pub name: String,
    pub version: String,
    pub agent_id: Uuid,
    pub agent_name: String,
    /// The LSL syntax id in use (nil: the bundled keywords).
    pub syntax_id: Uuid,
    /// Where the external files are (script.list's temp_dir).
    pub temp_dir: String,
}

/// A script edited externally now: its id, prim, item and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub id: String,
    pub object: Uuid,
    pub item: Uuid,
    /// The linkset's root (the prim itself when unknown).
    pub root: Uuid,
    pub name: String,
}

/// The challenge of one connection: the secret and the file holding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub secret: Uuid,
    pub file: PathBuf,
}

/// What the link must do for the protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    /// Send this text message on the connection.
    Send(u32, String),
    /// Close the connection with this code and reason.
    Close(u32, u16, String),
    /// The challenge file has done its work: delete it.
    Remove(PathBuf),
}

enum State {
    /// The handshake call is out with this request id.
    Waiting {
        request: u64,
        challenge: Challenge,
        deadline: Instant,
    },
    Ready,
    Closing,
}

struct Subscription {
    edited: Edited,
    connection: Option<u32>,
}

/// The link's protocol state: its connections and the scripts edited
/// externally.
#[derive(Default)]
pub struct Hub {
    connections: BTreeMap<u32, State>,
    subscriptions: BTreeMap<String, Subscription>,
    /// The scripts already offered: one a client unsubscribed from is not
    /// offered again until its external editing starts over (the Legacy
    /// Viewer forgets the editor then, unsubscribeEditor).
    offered: BTreeSet<String>,
    next_request: u64,
    syntax_id: Option<Uuid>,
    pub out: Vec<Out>,
}

fn request(id: u64, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn notification(method: &str, params: Option<Value>) -> String {
    match params {
        Some(params) => json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        None => json!({ "jsonrpc": "2.0", "method": method }),
    }
    .to_string()
}

fn result(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

impl Hub {
    /// Whether a connection got through the handshake.
    pub fn ready(&self, connection: u32) -> bool {
        matches!(self.connections.get(&connection), Some(State::Ready))
    }

    /// A new connection: the handshake call with its challenge, or, when
    /// no challenge could be made, the connection's end.
    pub fn opened(&mut self, connection: u32, challenge: Option<Challenge>, viewer: &Viewer, now: Instant) {
        let Some(challenge) = challenge else {
            self.disconnect(connection, Disconnect::InternalError, "Unable to issue a challenge");
            return;
        };
        self.next_request += 1;
        let id = self.next_request;
        let params = json!({
            "server_version": SERVER_VERSION,
            "protocol_version": PROTOCOL_VERSION,
            "viewer_name": viewer.name,
            "viewer_version": viewer.version,
            "agent_id": viewer.agent_id.to_string(),
            "agent_name": viewer.agent_name,
            "challenge": challenge.file.to_string_lossy(),
            "languages": ["lsl"],
            "syntax_id": viewer.syntax_id.to_string(),
            "features": {
                "live_sync": true,
                "compilation": true,
                "syntax_cache": false,
                "commands": false,
                "unified_diagnostics": true,
            },
        });
        self.out.push(Out::Send(connection, request(id, "session.handshake", params)));
        self.connections.insert(
            connection,
            State::Waiting {
                request: id,
                challenge,
                deadline: now + HANDSHAKE_TIMEOUT,
            },
        );
    }

    /// The connection is gone: its challenge file goes, its scripts wait
    /// for another (unsubscribeConnection).
    pub fn closed(&mut self, connection: u32) {
        if let Some(State::Waiting { challenge, .. }) = self.connections.remove(&connection) {
            self.out.push(Out::Remove(challenge.file));
        }
        for sub in self.subscriptions.values_mut() {
            if sub.connection == Some(connection) {
                sub.connection = None;
            }
        }
    }

    /// session.disconnect, then the close (sendDisconnect).
    fn disconnect(&mut self, connection: u32, reason: Disconnect, message: &str) {
        if let Some(State::Waiting { challenge, .. }) = self.connections.insert(connection, State::Closing) {
            self.out.push(Out::Remove(challenge.file));
        }
        let params = json!({ "reason": reason as i64, "message": message });
        self.out
            .push(Out::Send(connection, notification("session.disconnect", Some(params))));
        self.out.push(Out::Close(connection, 1000, message.into()));
    }

    /// The link stops: the challenge files still out go.
    pub fn shutdown(&mut self) {
        for state in std::mem::take(&mut self.connections).into_values() {
            if let State::Waiting { challenge, .. } = state {
                self.out.push(Out::Remove(challenge.file));
            }
        }
    }

    /// Handshakes unanswered in time end.
    pub fn tick(&mut self, now: Instant) {
        let late: Vec<u32> = self
            .connections
            .iter()
            .filter(|(_, s)| matches!(s, State::Waiting { deadline, .. } if now >= *deadline))
            .map(|(c, _)| *c)
            .collect();
        for connection in late {
            self.disconnect(connection, Disconnect::Timeout, "Handshake failed");
        }
    }

    /// The scripts edited externally now. New ones can be subscribed to;
    /// those no longer edited tell their client (script.unsubscribe) and
    /// go. A change of syntax id is told to every client.
    pub fn editing(&mut self, edited: &[Edited], viewer: &Viewer) {
        let ids: BTreeSet<&str> = edited.iter().map(|e| e.id.as_str()).collect();
        let gone: Vec<String> = self.offered.iter().filter(|id| !ids.contains(id.as_str())).cloned().collect();
        for id in gone {
            self.offered.remove(&id);
            if let Some(sub) = self.subscriptions.remove(&id)
                && let Some(connection) = sub.connection
                && self.ready(connection)
            {
                let params = json!({ "script_id": id });
                self.out
                    .push(Out::Send(connection, notification("script.unsubscribe", Some(params))));
            }
        }
        for e in edited {
            if self.offered.insert(e.id.clone()) {
                self.subscriptions.insert(
                    e.id.clone(),
                    Subscription {
                        edited: e.clone(),
                        connection: None,
                    },
                );
            } else if let Some(sub) = self.subscriptions.get_mut(&e.id) {
                sub.edited = e.clone();
            }
        }
        let syntax = Some(viewer.syntax_id);
        if self.syntax_id.is_some() && self.syntax_id != syntax {
            let message = notification("language.syntax.change", Some(json!({ "id": viewer.syntax_id.to_string() })));
            for connection in self.ready_connections() {
                self.out.push(Out::Send(connection, message.clone()));
            }
        }
        self.syntax_id = syntax;
    }

    fn ready_connections(&self) -> Vec<u32> {
        self.connections
            .iter()
            .filter(|(_, s)| matches!(s, State::Ready))
            .map(|(c, _)| *c)
            .collect()
    }

    /// A script's save came back: its compile result goes to the client
    /// subscribed to it (sendCompileResults). `errors` are the grid's
    /// lines, "(line, column) : LEVEL : message", 0-based.
    pub fn compiled(&mut self, id: &str, success: bool, running: bool, errors: &[String]) {
        let Some(connection) = self.subscriptions.get(id).and_then(|s| s.connection) else {
            return;
        };
        if !self.ready(connection) {
            return;
        }
        let mut params = json!({ "script_id": id, "success": success, "running": running });
        if !errors.is_empty() {
            params["diagnostics"] = Value::Array(errors.iter().map(|e| diagnostic(e)).collect());
        }
        self.out.push(Out::Send(connection, notification("script.compiled", Some(params))));
    }

    /// A text message from a client.
    pub fn received(&mut self, connection: u32, text: &str, viewer: &Viewer) {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            if self.ready(connection) {
                self.out
                    .push(Out::Send(connection, error(&Value::Null, code::PARSE, "Parse error")));
            }
            return;
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match message.get("method").and_then(Value::as_str) {
            Some(method) => match id {
                Some(id) => self.call(connection, &id, method, &params, viewer),
                None if method == "session.disconnect" => {
                    self.connections.insert(connection, State::Closing);
                    self.out.push(Out::Close(connection, 1000, "Goodbye".into()));
                }
                None => {}
            },
            None => {
                if let Some(id) = id {
                    self.answer(connection, &id, &message);
                } else if self.ready(connection) {
                    self.out
                        .push(Out::Send(connection, error(&Value::Null, code::INVALID_REQUEST, "Invalid Request")));
                }
            }
        }
    }

    /// The client's answer to one of our calls: only the handshake's is
    /// awaited (handleHandshakeResponse).
    fn answer(&mut self, connection: u32, id: &Value, message: &Value) {
        let Some(State::Waiting { request, challenge, .. }) = self.connections.get(&connection) else {
            return;
        };
        if id.as_u64() != Some(*request) {
            return;
        }
        let secret = challenge.secret;
        let file = challenge.file.clone();
        if message.get("error").is_some() {
            self.disconnect(connection, Disconnect::ProtocolError, "Handshake failed");
            return;
        }
        let answered = message
            .get("result")
            .and_then(|r| r.get("challenge_response"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s.trim()).ok());
        if answered != Some(secret) {
            self.disconnect(connection, Disconnect::ProtocolError, "Invalid challenge response");
            return;
        }
        self.out.push(Out::Remove(file));
        self.connections.insert(connection, State::Ready);
        self.out.push(Out::Send(connection, notification("session.ok", None)));
    }

    fn call(&mut self, connection: u32, id: &Value, method: &str, params: &Value, viewer: &Viewer) {
        match self.connections.get(&connection) {
            Some(State::Ready) => {}
            Some(State::Waiting { .. }) => {
                self.out.push(Out::Send(
                    connection,
                    error(id, code::AUTHENTICATION_REQUIRED, "Authentication required"),
                ));
                return;
            }
            _ => return,
        }
        let reply = match method {
            "system.ping" => {
                let mut r = json!({ "pong": "pong", "server_time": unix_millis() });
                if let Some(t) = params.get("timestamp") {
                    r["timestamp"] = t.clone();
                }
                Ok(r)
            }
            "system.getVersion" => Ok(json!({ "client_name": viewer.name, "client_version": viewer.version })),
            "system.status" => Ok(json!({ "status": "OK" })),
            "system.listMethods" => Ok(json!(METHODS)),
            "script.subscribe" => match params.get("script_id").and_then(Value::as_str) {
                Some(script) => Ok(self.subscribe(connection, script)),
                None => Err((code::INVALID_PARAMS, "Missing script_id")),
            },
            "script.unsubscribe" => {
                // idempotent: another's or an unknown script is no error
                if let Some(script) = params.get("script_id").and_then(Value::as_str)
                    && self.subscriptions.get(script).is_some_and(|s| s.connection == Some(connection))
                {
                    self.subscriptions.remove(script);
                }
                Ok(Value::Null)
            }
            "script.list" => Ok(json!({
                "temp_dir": viewer.temp_dir,
                "script_ids": self.subscriptions.keys().collect::<Vec<_>>(),
                "success": true,
            })),
            "language.syntax.id" => Ok(json!({ "id": viewer.syntax_id.to_string() })),
            _ => Err((code::METHOD_NOT_FOUND, "Method not found")),
        };
        let text = match reply {
            Ok(value) => result(id, value),
            Err((code, message)) => error(id, code, message),
        };
        self.out.push(Out::Send(connection, text));
    }

    /// handleScriptSubscribe / updateScriptSubscription.
    fn subscribe(&mut self, connection: u32, script: &str) -> Value {
        let status = match self.subscriptions.get_mut(script) {
            None => Subscribe::InvalidSubscription,
            Some(sub) => match sub.connection {
                Some(other) if other != connection => Subscribe::AlreadySubscribed,
                _ => {
                    sub.connection = Some(connection);
                    Subscribe::Success
                }
            },
        };
        let mut reply = json!({
            "script_id": script,
            "success": status == Subscribe::Success,
            "status": status as i64,
            "message": status.message(),
        });
        if status == Subscribe::Success
            && let Some(sub) = self.subscriptions.get(script)
        {
            reply["object_id"] = json!(sub.edited.object.to_string());
            reply["root_id"] = json!(sub.edited.root.to_string());
            reply["item_id"] = json!(sub.edited.item.to_string());
        }
        reply
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// One LSL compile error as a diagnostic: "(line, column) : LEVEL :
/// message", 0-based, gives a 1-based row and column; anything else is an
/// ERROR at row 0 (LSL_LOCATION_PATTERN in sendCompileResults).
pub fn diagnostic(line: &str) -> Value {
    let parsed = (|| {
        let rest = line.trim_start().strip_prefix('(')?;
        let (row, rest) = rest.split_once(", ")?;
        let (column, rest) = rest.split_once(") : ")?;
        let (level, message) = rest.split_once(" : ")?;
        let row: u32 = row.parse().ok()?;
        let column: u32 = column.parse().ok()?;
        if level.is_empty() || level.contains(':') || message.is_empty() {
            return None;
        }
        Some((row.checked_add(1)?, column.checked_add(1)?, level.to_owned(), message.to_owned()))
    })();
    let (row, column, level, message) = parsed.unwrap_or_else(|| (0, 0, "ERROR".into(), line.to_owned()));
    json!({ "row": row, "column": column, "level": level, "message": message, "format": "lsl" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewer() -> Viewer {
        Viewer {
            name: "Aurora".into(),
            version: "1.2.3".into(),
            agent_id: Uuid::from_u128(7),
            agent_name: "resident".into(),
            syntax_id: Uuid::nil(),
            temp_dir: "/tmp".into(),
        }
    }

    fn challenge() -> Challenge {
        Challenge {
            secret: Uuid::from_u128(42),
            file: PathBuf::from("/tmp/sl_script_challenge_x.tmp"),
        }
    }

    fn sent(hub: &mut Hub) -> Vec<Value> {
        hub.out
            .drain(..)
            .filter_map(|o| match o {
                Out::Send(_, text) => serde_json::from_str(&text).ok(),
                _ => None,
            })
            .collect()
    }

    /// A connection let in: (hub, connection 1).
    fn ready_hub(now: Instant) -> Hub {
        let mut hub = Hub::default();
        hub.opened(1, Some(challenge()), &viewer(), now);
        let call = sent(&mut hub).remove(0);
        let id = call["id"].clone();
        let answer = json!({ "jsonrpc": "2.0", "id": id, "result": {
            "client_name": "vscode", "protocol_version": "1.0",
            "challenge_response": Uuid::from_u128(42).to_string() } });
        hub.received(1, &answer.to_string(), &viewer());
        assert!(hub.ready(1));
        hub.out.clear();
        hub
    }

    fn edited(n: u128) -> Edited {
        Edited {
            id: script_id(Uuid::from_u128(n), Uuid::from_u128(n + 100)),
            object: Uuid::from_u128(n),
            item: Uuid::from_u128(n + 100),
            root: Uuid::from_u128(1000),
            name: "Script".into(),
        }
    }

    #[test]
    fn script_ids_and_file_names_follow_the_legacy_viewer() {
        let object = Uuid::nil();
        let item = Uuid::from_u128(1);
        let id = script_id(object, item);
        assert_eq!(
            id,
            format!(
                "{:x}",
                md5::compute("00000000-0000-0000-0000-000000000000_00000000-0000-0000-0000-000000000001")
            )
        );
        assert_eq!(id.len(), 32);
        assert_eq!(file_name("door", object, item), format!("sl_script_door_{id}.lsl"));
        assert_eq!(file_name("", object, item), format!("sl_script_{id}.lsl"));
    }

    #[test]
    fn handshake_carries_the_challenge_and_only_the_secret_lets_in() {
        let now = Instant::now();
        let mut hub = Hub::default();
        hub.opened(1, Some(challenge()), &viewer(), now);
        let call = sent(&mut hub).remove(0);
        assert_eq!(call["method"], "session.handshake");
        assert_eq!(call["params"]["challenge"], "/tmp/sl_script_challenge_x.tmp");
        assert_eq!(call["params"]["protocol_version"], "1.0");
        assert_eq!(call["params"]["features"]["live_sync"], true);
        assert_eq!(call["params"]["features"]["commands"], false);
        // a request before the answer is refused
        hub.received(1, r#"{"jsonrpc":"2.0","id":5,"method":"script.list"}"#, &viewer());
        assert_eq!(sent(&mut hub)[0]["error"]["code"], code::AUTHENTICATION_REQUIRED);
        // the wrong secret closes it, and the file goes
        let wrong = json!({ "jsonrpc": "2.0", "id": call["id"], "result": { "challenge_response": Uuid::from_u128(41).to_string() } });
        hub.received(1, &wrong.to_string(), &viewer());
        assert!(!hub.ready(1));
        assert!(hub.out.contains(&Out::Remove(challenge().file)));
        assert!(hub.out.iter().any(|o| matches!(o, Out::Close(1, 1000, _))));
        let note = sent(&mut hub);
        assert_eq!(note[0]["method"], "session.disconnect");
        assert_eq!(note[0]["params"]["reason"], Disconnect::ProtocolError as i64);
        // the right one, any case
        let mut hub = Hub::default();
        hub.opened(2, Some(challenge()), &viewer(), now);
        let call = sent(&mut hub).remove(0);
        let right = json!({ "jsonrpc": "2.0", "id": call["id"], "result": {
            "challenge_response": Uuid::from_u128(42).to_string().to_uppercase() } });
        hub.received(2, &right.to_string(), &viewer());
        assert!(hub.ready(2));
        assert!(hub.out.contains(&Out::Remove(challenge().file)));
        assert_eq!(sent(&mut hub)[0]["method"], "session.ok");
    }

    #[test]
    fn handshake_times_out_and_no_challenge_means_no_session() {
        let now = Instant::now();
        let mut hub = Hub::default();
        hub.opened(1, Some(challenge()), &viewer(), now);
        hub.out.clear();
        hub.tick(now + Duration::from_secs(29));
        assert!(hub.out.is_empty());
        hub.tick(now + HANDSHAKE_TIMEOUT);
        assert!(hub.out.contains(&Out::Remove(challenge().file)));
        assert_eq!(sent(&mut hub)[0]["params"]["reason"], Disconnect::Timeout as i64);
        let mut hub = Hub::default();
        hub.opened(3, None, &viewer(), now);
        assert!(hub.out.iter().any(|o| matches!(o, Out::Close(3, ..))));
        assert_eq!(sent(&mut hub)[0]["params"]["reason"], Disconnect::InternalError as i64);
        // a connection closed while waiting leaves no file behind
        let mut hub = Hub::default();
        hub.opened(4, Some(challenge()), &viewer(), now);
        hub.out.clear();
        hub.closed(4);
        assert_eq!(hub.out, vec![Out::Remove(challenge().file)]);
    }

    #[test]
    fn subscriptions_route_compile_results_to_their_client_only() {
        let now = Instant::now();
        let mut hub = ready_hub(now);
        let a = edited(1);
        hub.editing(std::slice::from_ref(&a), &viewer());
        let call = |hub: &mut Hub, c: u32, method: &str, params: Value| {
            let text = json!({ "jsonrpc": "2.0", "id": 9, "method": method, "params": params }).to_string();
            hub.received(c, &text, &viewer());
            sent(hub).remove(0)
        };
        let unknown = call(&mut hub, 1, "script.subscribe", json!({ "script_id": "nope" }));
        assert_eq!(unknown["result"]["status"], Subscribe::InvalidSubscription as i64);
        let ok = call(&mut hub, 1, "script.subscribe", json!({ "script_id": a.id }));
        assert_eq!(ok["result"]["success"], true);
        assert_eq!(ok["result"]["item_id"], a.item.to_string());
        assert_eq!(ok["result"]["root_id"], Uuid::from_u128(1000).to_string());
        let list = call(&mut hub, 1, "script.list", Value::Null);
        assert_eq!(list["result"]["script_ids"][0], a.id);
        // a second client cannot take it
        hub.opened(2, Some(challenge()), &viewer(), now);
        let handshake = sent(&mut hub).remove(0);
        let answer =
            json!({ "jsonrpc": "2.0", "id": handshake["id"], "result": { "challenge_response": Uuid::from_u128(42).to_string() } });
        hub.received(2, &answer.to_string(), &viewer());
        hub.out.clear();
        let taken = call(&mut hub, 2, "script.subscribe", json!({ "script_id": a.id }));
        assert_eq!(taken["result"]["status"], Subscribe::AlreadySubscribed as i64);
        assert!(taken["result"].get("item_id").is_none());
        hub.compiled(&a.id, false, true, &["(4, 10) : ERROR : Syntax error".into()]);
        match &hub.out[..] {
            [Out::Send(1, text)] => {
                let v: Value = serde_json::from_str(text).unwrap();
                assert_eq!(v["method"], "script.compiled");
                assert_eq!(v["params"]["success"], false);
                assert_eq!(v["params"]["diagnostics"][0]["row"], 5);
                assert_eq!(v["params"]["diagnostics"][0]["column"], 11);
            }
            other => panic!("{other:?}"),
        }
        hub.out.clear();
        // its client gone, another may take it
        hub.closed(1);
        let retaken = call(&mut hub, 2, "script.subscribe", json!({ "script_id": a.id }));
        assert_eq!(retaken["result"]["success"], true);
        // the editor closes: the client hears it
        hub.editing(&[], &viewer());
        let gone = sent(&mut hub);
        assert_eq!(gone[0]["method"], "script.unsubscribe");
        assert_eq!(gone[0]["params"]["script_id"], a.id);
        hub.compiled(&a.id, true, true, &[]);
        assert!(hub.out.is_empty());
    }

    #[test]
    fn unsubscribe_is_idempotent_and_forgets_until_editing_starts_over() {
        let now = Instant::now();
        let mut hub = ready_hub(now);
        let a = edited(1);
        hub.editing(std::slice::from_ref(&a), &viewer());
        let send = |hub: &mut Hub, method: &str, id: &str| {
            let text = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": { "script_id": id } }).to_string();
            hub.received(1, &text, &viewer());
            sent(hub).remove(0)
        };
        assert_eq!(send(&mut hub, "script.unsubscribe", "unknown")["result"], Value::Null);
        assert_eq!(send(&mut hub, "script.subscribe", &a.id)["result"]["success"], true);
        assert_eq!(send(&mut hub, "script.unsubscribe", &a.id)["result"], Value::Null);
        hub.editing(std::slice::from_ref(&a), &viewer());
        assert_eq!(send(&mut hub, "script.subscribe", &a.id)["result"]["success"], false);
        hub.editing(&[], &viewer());
        hub.editing(std::slice::from_ref(&a), &viewer());
        assert_eq!(send(&mut hub, "script.subscribe", &a.id)["result"]["success"], true);
    }

    #[test]
    fn system_methods_unknown_methods_and_bad_json() {
        let mut hub = ready_hub(Instant::now());
        hub.received(
            1,
            r#"{"jsonrpc":"2.0","id":"p","method":"system.ping","params":{"timestamp":12}}"#,
            &viewer(),
        );
        hub.received(1, r#"{"jsonrpc":"2.0","id":2,"method":"object.list"}"#, &viewer());
        hub.received(1, "not json", &viewer());
        hub.received(1, r#"{"jsonrpc":"2.0","id":3,"method":"system.listMethods"}"#, &viewer());
        let out = sent(&mut hub);
        assert_eq!(out[0]["id"], "p");
        assert_eq!(out[0]["result"]["pong"], "pong");
        assert_eq!(out[0]["result"]["timestamp"], 12);
        assert_eq!(out[1]["error"]["code"], code::METHOD_NOT_FOUND);
        assert_eq!(out[2]["error"]["code"], code::PARSE);
        assert!(out[3]["result"].as_array().unwrap().contains(&json!("script.subscribe")));
        hub.received(
            1,
            r#"{"jsonrpc":"2.0","method":"session.disconnect","params":{"reason":0}}"#,
            &viewer(),
        );
        assert!(hub.out.iter().any(|o| matches!(o, Out::Close(1, 1000, _))));
        assert!(!hub.ready(1));
    }

    #[test]
    fn syntax_changes_are_told_to_every_client() {
        let mut hub = ready_hub(Instant::now());
        let mut v = viewer();
        hub.editing(&[], &v);
        assert!(hub.out.is_empty());
        v.syntax_id = Uuid::from_u128(9);
        hub.editing(&[], &v);
        let out = sent(&mut hub);
        assert_eq!(out[0]["method"], "language.syntax.change");
        assert_eq!(out[0]["params"]["id"], Uuid::from_u128(9).to_string());
    }

    #[test]
    fn diagnostics_parse_the_grids_lines() {
        let d = diagnostic("(0, 4) : ERROR : Name not defined within scope");
        assert_eq!((d["row"].as_u64(), d["column"].as_u64()), (Some(1), Some(5)));
        assert_eq!(d["level"], "ERROR");
        assert_eq!(d["message"], "Name not defined within scope");
        let w = diagnostic("(12, 0) : WARNING : unused");
        assert_eq!(w["level"], "WARNING");
        let raw = diagnostic("Compile timed out");
        assert_eq!((raw["row"].as_u64(), raw["level"].as_str()), (Some(0), Some("ERROR")));
        assert_eq!(raw["message"], "Compile timed out");
    }
}
