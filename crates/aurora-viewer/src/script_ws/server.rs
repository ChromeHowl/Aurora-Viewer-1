//! The script editor link's sockets: a listener on 127.0.0.1 only, one
//! thread per connection, and the challenge files. The protocol itself is
//! `protocol::Hub`, run on the UI thread by [`Server::poll`].

use super::{
    protocol::{Challenge, Edited, Hub, Out, Viewer},
    wire,
};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::{
    io::{ErrorKind, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

/// Connections served at once; more are closed at once.
const MAX_CONNECTIONS: usize = 8;
/// How long a connection's thread waits for bytes before it looks at what
/// it has to send.
const POLL: Duration = Duration::from_millis(30);

enum Event {
    Opened(u32, Sender<Out>),
    Text(u32, String),
    Closed(u32),
}

/// The running link. Dropping it closes the listener and every connection.
pub struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
    events: Receiver<Event>,
    connections: std::collections::BTreeMap<u32, Sender<Out>>,
    hub: Hub,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.hub.shutdown();
        for out in std::mem::take(&mut self.hub.out) {
            if let Out::Remove(file) = out {
                remove(&file);
            }
        }
    }
}

impl Server {
    /// Listens on 127.0.0.1:`port`: never on another address, whatever the
    /// Legacy Viewer's ExternalWebsocketSyncLocal allows.
    pub fn start(port: u16) -> std::io::Result<Server> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, events) = unbounded();
        let thread_stop = stop.clone();
        std::thread::Builder::new()
            .name("script-link".into())
            .spawn(move || accept(listener, tx, thread_stop))?;
        log::info!("script editor link listening on 127.0.0.1:{port}");
        Ok(Server {
            port,
            stop,
            events,
            connections: Default::default(),
            hub: Hub::default(),
        })
    }

    /// One frame: the clients' messages, the handshakes' time, the scripts
    /// edited externally now.
    pub fn poll(&mut self, viewer: &Viewer, edited: &[Edited], now: Instant) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Opened(id, tx) => {
                    self.connections.insert(id, tx);
                    self.hub.opened(id, write_challenge(), viewer, now);
                }
                Event::Text(id, text) => self.hub.received(id, &text, viewer),
                Event::Closed(id) => {
                    self.connections.remove(&id);
                    self.hub.closed(id);
                }
            }
        }
        self.hub.tick(now);
        self.hub.editing(edited, viewer);
        self.flush();
    }

    /// A script's save came back (see `Hub::compiled`).
    pub fn compiled(&mut self, id: &str, success: bool, running: bool, errors: &[String]) {
        self.hub.compiled(id, success, running, errors);
        self.flush();
    }

    fn flush(&mut self) {
        for out in std::mem::take(&mut self.hub.out) {
            match out {
                Out::Remove(file) => remove(&file),
                Out::Send(id, _) | Out::Close(id, ..) => {
                    if let Some(tx) = self.connections.get(&id) {
                        let _ = tx.send(out);
                    }
                }
            }
        }
    }
}

fn remove(file: &Path) {
    if let Err(e) = std::fs::remove_file(file)
        && e.kind() != ErrorKind::NotFound
    {
        log::warn!("script editor link: challenge file not removed: {e}");
    }
}

/// writeChallenge: a random secret (the system's random source, through
/// UUID v4) in a new file of the temporary folder, under a name that says
/// nothing of it, readable by the user alone. Never written over an
/// existing file or through a link (`create_new`). Windows' temporary
/// folder is the user's own.
fn write_challenge() -> Option<Challenge> {
    let secret = Uuid::new_v4();
    let file = std::env::temp_dir().join(format!("sl_script_challenge_{}.tmp", Uuid::new_v4()));
    match write_user_only(&file, &secret.to_string()) {
        Ok(()) => Some(Challenge { secret, file }),
        Err(e) => {
            log::warn!("script editor link: no challenge file: {e}");
            None
        }
    }
}

fn write_user_only(path: &PathBuf, text: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

fn accept(listener: TcpListener, events: Sender<Event>, stop: Arc<AtomicBool>) {
    let open = Arc::new(AtomicUsize::new(0));
    let mut next: u32 = 0;
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                // bound to the loopback address: nothing else can come in
                if !peer.ip().is_loopback() || open.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    drop(stream);
                    continue;
                }
                next = next.wrapping_add(1).max(1);
                let (id, events, stop, open) = (next, events.clone(), stop.clone(), open.clone());
                open.fetch_add(1, Ordering::Relaxed);
                let spawned = std::thread::Builder::new().name("script-link-client".into()).spawn(move || {
                    serve(stream, id, &events, &stop);
                    let _ = events.send(Event::Closed(id));
                    open.fetch_sub(1, Ordering::Relaxed);
                });
                if let Err(e) = spawned {
                    log::warn!("script editor link: connection refused: {e}");
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                log::warn!("script editor link: accept failed: {e}");
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

/// One connection: the upgrade, then messages both ways until either side
/// closes or the link stops.
fn serve(mut stream: TcpStream, id: u32, events: &Sender<Event>, stop: &AtomicBool) {
    if stream.set_nonblocking(false).is_err() || stream.set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let _ = stream.set_nodelay(true);
    let mut request = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    let started = Instant::now();
    let rest = loop {
        if stop.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(10) {
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => request.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => continue,
            Err(_) => return,
        }
        match wire::upgrade(&request) {
            wire::Upgrade::Partial => {}
            wire::Upgrade::Refuse(reply) => {
                let _ = stream.write_all(&reply);
                return;
            }
            wire::Upgrade::Accept { reply, used } => {
                if stream.write_all(&reply).is_err() {
                    return;
                }
                break request.split_off(used);
            }
        }
    };
    let (tx, outgoing) = unbounded();
    if events.send(Event::Opened(id, tx)).is_err() {
        return;
    }
    let mut decoder = wire::Decoder::default();
    decoder.push(&rest);
    loop {
        loop {
            match decoder.next() {
                Ok(None) => break,
                Ok(Some(wire::Message::Text(text))) => {
                    if events.send(Event::Text(id, text)).is_err() {
                        return;
                    }
                }
                Ok(Some(wire::Message::Ping(payload))) => {
                    if stream.write_all(&wire::pong(&payload)).is_err() {
                        return;
                    }
                }
                Ok(Some(wire::Message::Pong)) => {}
                Ok(Some(wire::Message::Close)) => {
                    let _ = stream.write_all(&wire::close(1000, ""));
                    return;
                }
                Err(e) => {
                    let code = match e {
                        wire::WireError::Protocol(code) => code,
                        wire::WireError::TooLarge => 1009,
                    };
                    let _ = stream.write_all(&wire::close(code, ""));
                    return;
                }
            }
        }
        for out in outgoing.try_iter() {
            let written = match out {
                Out::Send(_, text) => stream.write_all(&wire::text(&text)),
                Out::Close(_, code, reason) => {
                    let _ = stream.write_all(&wire::close(code, &reason));
                    return;
                }
                Out::Remove(_) => Ok(()),
            };
            if written.is_err() {
                return;
            }
        }
        if stop.load(Ordering::Relaxed) {
            let _ = stream.write_all(&wire::close(1001, "Viewer closing"));
            return;
        }
        match stream.read(&mut chunk) {
            Ok(0) => return,
            Ok(n) => decoder.push(&chunk[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(_) => return,
        }
    }
}
