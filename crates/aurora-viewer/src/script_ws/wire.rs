//! The WebSocket wire format (RFC 6455) the script editor link speaks: the
//! HTTP upgrade of a new connection and the frames after it. Server side
//! only, text messages only, as the Legacy Viewer's LLWebsocketMgr serves
//! its JSON-RPC (indra/llcorehttp websocket server, originally LGPL 2.1).
//!
//! Pure: bytes in, bytes out, so every rule is tested without a socket.

use base64::Engine;
use sha1::{Digest, Sha1};

/// The GUID RFC 6455 section 1.3 appends to the client's key.
const ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// The largest upgrade request read before the connection is dropped.
pub const MAX_REQUEST: usize = 8 * 1024;
/// The largest message taken from a client (a script is 1 MB at most, and
/// nothing the link accepts carries one).
pub const MAX_MESSAGE: usize = 2 * 1024 * 1024;

/// What the upgrade request asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Upgrade {
    /// More bytes are needed.
    Partial,
    /// Accepted: the bytes to answer with, and how many bytes of the input
    /// the request took.
    Accept { reply: Vec<u8>, used: usize },
    /// Refused: the reply to send before closing.
    Refuse(Vec<u8>),
}

/// The `Sec-WebSocket-Accept` value for a client key.
pub fn accept_key(key: &str) -> String {
    let mut hash = Sha1::new();
    hash.update(key.trim().as_bytes());
    hash.update(ACCEPT_GUID.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hash.finalize())
}

fn refuse(status: &str) -> Upgrade {
    Upgrade::Refuse(format!("HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").into_bytes())
}

/// Reads the HTTP upgrade request at the start of `input`. A request with
/// an `Origin` header comes from a web page: it is refused (403), so no
/// page a browser shows can reach the link (the Legacy Viewer's rule,
/// LLScriptEditorWSServer).
pub fn upgrade(input: &[u8]) -> Upgrade {
    let Some(end) = input.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if input.len() > MAX_REQUEST {
            refuse("431 Request Header Fields Too Large")
        } else {
            Upgrade::Partial
        };
    };
    let Ok(head) = std::str::from_utf8(&input[..end]) else {
        return refuse("400 Bad Request");
    };
    let mut lines = head.split("\r\n");
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split(' ');
    if parts.next() != Some("GET") || !parts.nth(1).is_some_and(|v| v.starts_with("HTTP/1.1")) {
        return refuse("400 Bad Request");
    }
    let mut key = None;
    let mut websocket = false;
    let mut version = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => return refuse("403 Forbidden"),
            "upgrade" => websocket = value.eq_ignore_ascii_case("websocket"),
            "sec-websocket-version" => version = value == "13",
            "sec-websocket-key" => key = Some(value.to_owned()),
            _ => {}
        }
    }
    let Some(key) = key.filter(|_| websocket) else {
        return refuse("400 Bad Request");
    };
    if !version {
        return Upgrade::Refuse(
            b"HTTP/1.1 426 Upgrade Required\r\nSec-WebSocket-Version: 13\r\nConnection: close\r\nContent-Length: 0\r\n\r\n".to_vec(),
        );
    }
    let reply = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(&key)
    );
    Upgrade::Accept {
        reply: reply.into_bytes(),
        used: end + 4,
    }
}

/// A whole message or control frame from the client.
#[derive(Debug, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Ping(Vec<u8>),
    Pong,
    Close,
}

/// Why the connection must end.
#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    /// Not a frame a client may send (unmasked, reserved bits, bad opcode,
    /// binary data, text that is not UTF-8): close code 1002 or 1007.
    Protocol(u16),
    /// Larger than [`MAX_MESSAGE`]: close code 1009.
    TooLarge,
}

/// Reassembles the client's frames into messages, as bytes arrive.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    /// The text of a fragmented message so far.
    partial: Option<Vec<u8>>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole message, if the bytes so far hold one.
    pub fn next(&mut self) -> Result<Option<Message>, WireError> {
        loop {
            let Some((fin, opcode, payload, used)) = frame(&self.buffer)? else {
                return Ok(None);
            };
            self.buffer.drain(..used);
            match opcode {
                0x0 => {
                    let Some(partial) = self.partial.as_mut() else {
                        return Err(WireError::Protocol(1002));
                    };
                    if partial.len() + payload.len() > MAX_MESSAGE {
                        return Err(WireError::TooLarge);
                    }
                    partial.extend_from_slice(&payload);
                    if fin {
                        let text = self.partial.take().unwrap_or_default();
                        return text_message(text).map(Some);
                    }
                }
                0x1 => {
                    if self.partial.is_some() {
                        return Err(WireError::Protocol(1002));
                    }
                    if fin {
                        return text_message(payload).map(Some);
                    }
                    self.partial = Some(payload);
                }
                0x8 => return Ok(Some(Message::Close)),
                0x9 => return Ok(Some(Message::Ping(payload))),
                0xA => return Ok(Some(Message::Pong)),
                // binary data and reserved opcodes: nothing the link takes
                0x2 => return Err(WireError::Protocol(1003)),
                _ => return Err(WireError::Protocol(1002)),
            }
            // a fragment that did not end the message: read on
        }
    }
}

fn text_message(bytes: Vec<u8>) -> Result<Message, WireError> {
    String::from_utf8(bytes).map(Message::Text).map_err(|_| WireError::Protocol(1007))
}

/// One frame at the start of `input`: (fin, opcode, unmasked payload,
/// bytes used), or None while incomplete.
fn frame(input: &[u8]) -> Result<Option<(bool, u8, Vec<u8>, usize)>, WireError> {
    if input.len() < 2 {
        return Ok(None);
    }
    let fin = input[0] & 0x80 != 0;
    if input[0] & 0x70 != 0 {
        return Err(WireError::Protocol(1002));
    }
    let opcode = input[0] & 0x0F;
    // RFC 6455 5.1: every frame from a client is masked.
    if input[1] & 0x80 == 0 {
        return Err(WireError::Protocol(1002));
    }
    let (length, mut at) = match input[1] & 0x7F {
        126 => {
            if input.len() < 4 {
                return Ok(None);
            }
            (u64::from(u16::from_be_bytes([input[2], input[3]])), 4)
        }
        127 => {
            if input.len() < 10 {
                return Ok(None);
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&input[2..10]);
            (u64::from_be_bytes(bytes), 10)
        }
        n => (u64::from(n), 2),
    };
    let control = opcode & 0x08 != 0;
    if control && (length > 125 || !fin) {
        return Err(WireError::Protocol(1002));
    }
    if length > MAX_MESSAGE as u64 {
        return Err(WireError::TooLarge);
    }
    let length = length as usize;
    if input.len() < at + 4 + length {
        return Ok(None);
    }
    let mask = [input[at], input[at + 1], input[at + 2], input[at + 3]];
    at += 4;
    let payload = input[at..at + length].iter().enumerate().map(|(i, b)| b ^ mask[i % 4]).collect();
    Ok(Some((fin, opcode, payload, at + length)))
}

/// A frame from the server (never masked, RFC 6455 5.1).
fn server_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 10);
    out.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => out.push(n as u8),
        n if n <= usize::from(u16::MAX) => {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(payload);
    out
}

pub fn text(message: &str) -> Vec<u8> {
    server_frame(0x1, message.as_bytes())
}

pub fn pong(payload: &[u8]) -> Vec<u8> {
    server_frame(0xA, payload)
}

/// A close frame with its status code and a short reason (at most 123
/// bytes, cut on a character boundary).
pub fn close(code: u16, reason: &str) -> Vec<u8> {
    let mut end = reason.len().min(123);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(&reason.as_bytes()[..end]);
    server_frame(0x8, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn masked(first: u8, payload: &[u8]) -> Vec<u8> {
        let mask = [1u8, 2, 3, 4];
        let mut out = vec![first];
        match payload.len() {
            n if n < 126 => out.push(0x80 | n as u8),
            n => {
                out.push(0x80 | 126);
                out.extend_from_slice(&(n as u16).to_be_bytes());
            }
        }
        out.extend_from_slice(&mask);
        out.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        out
    }

    #[test]
    fn accept_key_matches_the_rfc_example() {
        assert_eq!(accept_key("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn upgrade_accepts_a_websocket_request_and_refuses_browsers() {
        let request = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:9020\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\nrest";
        match upgrade(request) {
            Upgrade::Accept { reply, used } => {
                let reply = String::from_utf8(reply).unwrap();
                assert!(reply.starts_with("HTTP/1.1 101"));
                assert!(reply.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n"));
                assert_eq!(&request[used..], b"rest");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(upgrade(&request[..20]), Upgrade::Partial);
        let browser = b"GET / HTTP/1.1\r\nOrigin: https://example.com\r\nUpgrade: websocket\r\nSec-WebSocket-Key: a\r\nSec-WebSocket-Version: 13\r\n\r\n";
        assert!(matches!(upgrade(browser), Upgrade::Refuse(r) if r.starts_with(b"HTTP/1.1 403")));
        let plain = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        assert!(matches!(upgrade(plain), Upgrade::Refuse(r) if r.starts_with(b"HTTP/1.1 400")));
        let old = b"GET / HTTP/1.1\r\nUpgrade: websocket\r\nSec-WebSocket-Key: a\r\nSec-WebSocket-Version: 8\r\n\r\n";
        assert!(matches!(upgrade(old), Upgrade::Refuse(r) if r.starts_with(b"HTTP/1.1 426")));
        assert!(matches!(upgrade(&vec![b'a'; MAX_REQUEST + 1]), Upgrade::Refuse(_)));
    }

    #[test]
    fn decoder_reassembles_split_and_fragmented_messages() {
        let mut d = Decoder::default();
        let bytes = masked(0x81, b"hello");
        d.push(&bytes[..3]);
        assert_eq!(d.next(), Ok(None));
        d.push(&bytes[3..]);
        assert_eq!(d.next(), Ok(Some(Message::Text("hello".into()))));
        d.push(&masked(0x01, b"ab"));
        d.push(&masked(0x89, b"p"));
        d.push(&masked(0x80, b"cd"));
        assert_eq!(d.next(), Ok(Some(Message::Ping(b"p".to_vec()))));
        assert_eq!(d.next(), Ok(Some(Message::Text("abcd".into()))));
        let long = vec![b'x'; 300];
        d.push(&masked(0x81, &long));
        assert_eq!(d.next(), Ok(Some(Message::Text("x".repeat(300)))));
        d.push(&masked(0x88, &[3, 232]));
        assert_eq!(d.next(), Ok(Some(Message::Close)));
    }

    #[test]
    fn decoder_refuses_what_a_client_may_not_send() {
        let mut d = Decoder::default();
        d.push(&[0x81, 0x02, b'h', b'i']);
        assert_eq!(d.next(), Err(WireError::Protocol(1002)), "unmasked");
        let mut d = Decoder::default();
        d.push(&masked(0x82, b"bin"));
        assert_eq!(d.next(), Err(WireError::Protocol(1003)), "binary");
        let mut d = Decoder::default();
        d.push(&masked(0x81, &[0xff, 0xfe]));
        assert_eq!(d.next(), Err(WireError::Protocol(1007)), "not UTF-8");
        let mut d = Decoder::default();
        d.push(&masked(0x80, b"x"));
        assert_eq!(d.next(), Err(WireError::Protocol(1002)), "stray continuation");
        let mut d = Decoder::default();
        let mut huge = vec![0x81, 0x80 | 127];
        huge.extend_from_slice(&(MAX_MESSAGE as u64 + 1).to_be_bytes());
        d.push(&huge);
        assert_eq!(d.next(), Err(WireError::TooLarge));
    }

    #[test]
    fn server_frames_are_unmasked_with_their_length() {
        assert_eq!(text("hi"), vec![0x81, 2, b'h', b'i']);
        let long = text(&"y".repeat(200));
        assert_eq!(&long[..4], &[0x81, 126, 0, 200]);
        assert_eq!(pong(b"p"), vec![0x8A, 1, b'p']);
        let c = close(1000, &"é".repeat(100));
        assert_eq!(&c[..4], &[0x88, 124, 0x03, 0xE8]);
    }
}
