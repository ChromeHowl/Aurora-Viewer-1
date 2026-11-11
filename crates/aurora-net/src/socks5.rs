//! SOCKS 5 proxy of the region traffic (RFC 1928, user name and password
//! of RFC 1929): the association opened before the login, and every
//! circuit packet wrapped on the way out and unwrapped on the way in.
//!
//! Port of LLProxy::startSOCKSProxy, proxyHandshake and the packet
//! wrapping of LLPacketRing::sendPacket / receivePacket
//! (indra/llmessage/llproxy.cpp, llpacketring.cpp, originally LGPL 2.1):
//! one method offered (none, or user name and password), then UDP
//! ASSOCIATE for 0.0.0.0:0, the control connection kept open for the whole
//! session. The HTTP traffic goes through the same proxy with reqwest's
//! own SOCKS 5 connector (`lib.rs`, `HttpProxy::Socks`).
//!
//! The user name and password are never logged.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const VERSION: u8 = 5;
const METHOD_NONE: u8 = 0x00;
const METHOD_USER_PASS: u8 = 0x02;
const METHOD_REFUSED: u8 = 0xff;
const USER_PASS_VERSION: u8 = 1;
const CMD_UDP_ASSOCIATE: u8 = 3;
const ATYP_IPV4: u8 = 1;
const ATYP_DOMAIN: u8 = 3;
const ATYP_IPV6: u8 = 4;
/// REP "connection not allowed by ruleset".
const REP_NOT_ALLOWED: u8 = 2;
/// How long the proxy may take for the whole handshake (the Legacy
/// Viewer's SOCKS_TIMEOUT is per step; one bound for all is enough here).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// A SOCKS 5 proxy (Socks5ProxyHost, Socks5ProxyPort, Socks5AuthType).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocksProxy {
    pub host: String,
    pub port: u16,
    /// Socks5AuthType "UserPass", with the credential the user saved.
    pub auth: Option<SocksAuth>,
}

/// The user name and password of a SOCKS 5 proxy (RFC 1929). Their
/// `Debug` hides them.
#[derive(Clone, PartialEq, Eq)]
pub struct SocksAuth {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for SocksAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SocksAuth(hidden)")
    }
}

impl SocksAuth {
    /// Both fit RFC 1929's one-byte lengths, and the user name is not empty
    /// (LLProxy::setAuthPassword refuses the rest: SOCKS_BAD_CREDS).
    pub fn valid(&self) -> bool {
        !self.username.is_empty() && self.username.len() <= 255 && self.password.len() <= 255
    }
}

/// Why the association failed, as the Legacy Viewer's notifications
/// name it (startLLProxy's SOCKS_* statuses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocksError {
    /// SOCKS_INVALID_HOST: the name resolves to no IPv4 address.
    InvalidHost,
    /// SOCKS_BAD_CREDS.
    BadCredentials,
    /// SOCKS_HOST_CONNECT_FAILED: no TCP connection to the proxy.
    HostConnectFailed,
    /// SOCKS_CONNECT_ERROR: the connection broke during the handshake, or
    /// the proxy did not answer in time.
    ConnectError,
    /// SOCKS_NOT_ACCEPTABLE: the proxy refused the authentication offered.
    NotAcceptable,
    /// SOCKS_AUTH_FAIL.
    AuthFail,
    /// SOCKS_NOT_PERMITTED: the proxy's rules refused the request.
    NotPermitted,
    /// SOCKS_UDP_FWD_NOT_GRANTED.
    UdpNotGranted,
    /// SOCKS_UNKNOWN_STATUS: an answer that is not SOCKS 5.
    Unknown,
}

impl SocksError {
    /// The text shown on the login screen.
    pub fn message(self, host: &str, port: u16) -> String {
        use aurora_i18n::tr;
        let (host, port) = (host.to_owned(), port.to_string());
        match self {
            SocksError::InvalidHost => tr!("l4-socks-invalid-host", host = host, port = port),
            SocksError::BadCredentials => tr!("l4-socks-bad-creds").to_owned(),
            SocksError::HostConnectFailed => tr!("l4-socks-host-connect-failed", host = host, port = port),
            SocksError::ConnectError => tr!("l4-socks-connect-error", host = host, port = port),
            SocksError::NotAcceptable => tr!("l4-socks-not-acceptable", host = host, port = port),
            SocksError::AuthFail => tr!("l4-socks-auth-fail", host = host, port = port),
            SocksError::NotPermitted => tr!("l4-socks-not-permitted", host = host, port = port),
            SocksError::UdpNotGranted => tr!("l4-socks-udp-not-granted", host = host, port = port),
            SocksError::Unknown => tr!("l4-socks-unknown", host = host, port = port),
        }
    }
}

/// The method negotiation: one method offered, as the Legacy Viewer does.
pub fn greeting(user_pass: bool) -> [u8; 3] {
    [VERSION, 1, if user_pass { METHOD_USER_PASS } else { METHOD_NONE }]
}

/// The proxy's choice of method, checked against the one offered.
pub fn check_method(reply: [u8; 2], user_pass: bool) -> Result<(), SocksError> {
    let wanted = if user_pass { METHOD_USER_PASS } else { METHOD_NONE };
    match reply {
        [VERSION, m] if m == wanted => Ok(()),
        [VERSION, _] => Err(SocksError::NotAcceptable),
        _ => Err(SocksError::Unknown),
    }
}

/// RFC 1929's request: version 1, then the user name and the password,
/// each after its length.
pub fn user_pass_request(auth: &SocksAuth) -> Result<Vec<u8>, SocksError> {
    if !auth.valid() {
        return Err(SocksError::BadCredentials);
    }
    let mut out = Vec::with_capacity(3 + auth.username.len() + auth.password.len());
    out.push(USER_PASS_VERSION);
    out.push(auth.username.len() as u8);
    out.extend_from_slice(auth.username.as_bytes());
    out.push(auth.password.len() as u8);
    out.extend_from_slice(auth.password.as_bytes());
    Ok(out)
}

/// RFC 1929's answer: status 0 is success.
pub fn check_user_pass(reply: [u8; 2]) -> Result<(), SocksError> {
    match reply {
        [_, 0] => Ok(()),
        _ => Err(SocksError::AuthFail),
    }
}

/// UDP ASSOCIATE for 0.0.0.0:0: the datagrams may come from any address
/// and port of ours (proxyHandshake's request).
pub fn udp_associate_request() -> [u8; 10] {
    [VERSION, CMD_UDP_ASSOCIATE, 0, ATYP_IPV4, 0, 0, 0, 0, 0, 0]
}

/// The first four bytes of the answer to UDP ASSOCIATE: its status, and
/// how many more bytes the bound address takes (port included); None for
/// a domain name, whose length comes in the next byte.
pub fn check_associate_head(head: [u8; 4]) -> Result<Option<usize>, SocksError> {
    if head[0] != VERSION {
        return Err(SocksError::Unknown);
    }
    match head[1] {
        0 => {}
        REP_NOT_ALLOWED => return Err(SocksError::NotPermitted),
        _ => return Err(SocksError::UdpNotGranted),
    }
    match head[3] {
        ATYP_IPV4 => Ok(Some(4 + 2)),
        ATYP_IPV6 => Ok(Some(16 + 2)),
        ATYP_DOMAIN => Ok(None),
        _ => Err(SocksError::Unknown),
    }
}

/// The relay the datagrams go to: the bound address of the answer, or the
/// proxy's own address when it answers 0.0.0.0 (RFC 1928 lets a server
/// mean "this host" so).
pub fn relay_address(atyp: u8, bound: &[u8], proxy: SocketAddr) -> Result<SocketAddr, SocksError> {
    let port_at = bound.len().checked_sub(2).ok_or(SocksError::Unknown)?;
    let port = u16::from_be_bytes([bound[port_at], bound[port_at + 1]]);
    let ip = match (atyp, &bound[..port_at]) {
        (ATYP_IPV4, a) if a.len() == 4 => IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3])),
        (ATYP_IPV6, a) if a.len() == 16 => {
            let mut b = [0u8; 16];
            b.copy_from_slice(a);
            IpAddr::V6(Ipv6Addr::from(b))
        }
        // a name: the proxy's own address serves (the circuits are IPv4)
        (ATYP_DOMAIN, _) => proxy.ip(),
        _ => return Err(SocksError::Unknown),
    };
    let ip = if ip.is_unspecified() { proxy.ip() } else { ip };
    if port == 0 {
        return Err(SocksError::UdpNotGranted);
    }
    Ok(SocketAddr::new(ip, port))
}

/// Wraps a circuit packet for the relay: RSV, FRAG 0, then the
/// simulator's address and port (RFC 1928 §7, LLPacketRing::sendPacket's
/// proxywrap_t). `out` is cleared first.
pub fn wrap(to: SocketAddr, payload: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(&[0, 0, 0]);
    match to.ip() {
        IpAddr::V4(ip) => {
            out.push(ATYP_IPV4);
            out.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            out.push(ATYP_IPV6);
            out.extend_from_slice(&ip.octets());
        }
    }
    out.extend_from_slice(&to.port().to_be_bytes());
    out.extend_from_slice(payload);
}

/// Unwraps a datagram from the relay: the simulator that sent it and where
/// its packet starts. None for a fragment (FRAG ≠ 0, which the Legacy
/// Viewer does not reassemble either), a name, or a short datagram.
pub fn unwrap(datagram: &[u8]) -> Option<(SocketAddr, usize)> {
    let (&[_, _, frag, atyp], rest) = datagram.split_first_chunk::<4>()?;
    if frag != 0 {
        return None;
    }
    let (ip, len) = match atyp {
        ATYP_IPV4 => {
            let a = rest.get(..4)?;
            (IpAddr::V4(Ipv4Addr::new(a[0], a[1], a[2], a[3])), 4)
        }
        ATYP_IPV6 => {
            let mut b = [0u8; 16];
            b.copy_from_slice(rest.get(..16)?);
            (IpAddr::V6(Ipv6Addr::from(b)), 16)
        }
        _ => return None,
    };
    let port = rest.get(len..len + 2)?;
    let port = u16::from_be_bytes([port[0], port[1]]);
    Some((SocketAddr::new(ip, port), 4 + len + 2))
}

/// An open association: the relay of the datagrams, and the control
/// connection, which keeps it alive while it stays open.
#[derive(Debug)]
pub struct UdpAssociation {
    pub relay: SocketAddr,
    pub control: TcpStream,
}

/// Opens the association (LLProxy::startSOCKSProxy).
pub async fn associate(proxy: &SocksProxy) -> Result<UdpAssociation, SocksError> {
    if proxy.auth.as_ref().is_some_and(|a| !a.valid()) {
        return Err(SocksError::BadCredentials);
    }
    if proxy.host.trim().is_empty() || proxy.port == 0 {
        return Err(SocksError::InvalidHost);
    }
    // LLHost::setHostByName: the first IPv4 address of the name
    let address = tokio::net::lookup_host((proxy.host.trim(), proxy.port))
        .await
        .map_err(|_| SocksError::InvalidHost)?
        .find(SocketAddr::is_ipv4)
        .ok_or(SocksError::InvalidHost)?;
    let mut control = tokio::time::timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| SocksError::HostConnectFailed)?
        .map_err(|_| SocksError::HostConnectFailed)?;
    let relay = tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake(&mut control, proxy.auth.as_ref(), address))
        .await
        .map_err(|_| SocksError::ConnectError)??;
    Ok(UdpAssociation { relay, control })
}

/// The handshake on an open control connection: method, credentials,
/// UDP ASSOCIATE; returns the relay.
async fn handshake<S>(stream: &mut S, auth: Option<&SocksAuth>, proxy: SocketAddr) -> Result<SocketAddr, SocksError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let io = |_| SocksError::ConnectError;
    stream.write_all(&greeting(auth.is_some())).await.map_err(io)?;
    let mut method = [0u8; 2];
    stream.read_exact(&mut method).await.map_err(io)?;
    if method == [VERSION, METHOD_REFUSED] {
        return Err(SocksError::NotAcceptable);
    }
    check_method(method, auth.is_some())?;
    if let Some(auth) = auth {
        stream.write_all(&user_pass_request(auth)?).await.map_err(io)?;
        let mut status = [0u8; 2];
        stream.read_exact(&mut status).await.map_err(io)?;
        check_user_pass(status)?;
    }
    stream.write_all(&udp_associate_request()).await.map_err(io)?;
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.map_err(io)?;
    let len = match check_associate_head(head)? {
        Some(len) => len,
        None => {
            let mut n = [0u8; 1];
            stream.read_exact(&mut n).await.map_err(io)?;
            usize::from(n[0]) + 2
        }
    };
    let mut bound = vec![0u8; len];
    stream.read_exact(&mut bound).await.map_err(io)?;
    relay_address(head[3], &bound, proxy)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(user: &str, pass: &str) -> SocksAuth {
        SocksAuth {
            username: user.into(),
            password: pass.into(),
        }
    }

    #[test]
    fn the_greeting_offers_one_method() {
        assert_eq!(greeting(false), [5, 1, 0]);
        assert_eq!(greeting(true), [5, 1, 2]);
        assert_eq!(check_method([5, 0], false), Ok(()));
        assert_eq!(check_method([5, 2], true), Ok(()));
        assert_eq!(check_method([5, 0], true), Err(SocksError::NotAcceptable));
        assert_eq!(check_method([5, 0xff], false), Err(SocksError::NotAcceptable));
        assert_eq!(check_method([4, 0], false), Err(SocksError::Unknown));
    }

    #[test]
    fn user_and_password_follow_rfc_1929() {
        assert_eq!(
            user_pass_request(&auth("ab", "xyz")).expect("valid"),
            vec![1, 2, b'a', b'b', 3, b'x', b'y', b'z']
        );
        assert_eq!(user_pass_request(&auth("", "x")), Err(SocksError::BadCredentials));
        assert_eq!(user_pass_request(&auth(&"u".repeat(256), "x")), Err(SocksError::BadCredentials));
        assert_eq!(user_pass_request(&auth("u", &"p".repeat(256))), Err(SocksError::BadCredentials));
        assert!(user_pass_request(&auth(&"u".repeat(255), "")).is_ok());
        assert_eq!(check_user_pass([1, 0]), Ok(()));
        assert_eq!(check_user_pass([1, 1]), Err(SocksError::AuthFail));
    }

    #[test]
    fn the_credentials_never_show_in_debug() {
        let text = format!(
            "{:?}",
            SocksProxy {
                host: "proxy".into(),
                port: 1080,
                auth: Some(auth("someone", "secret")),
            }
        );
        assert!(!text.contains("someone") && !text.contains("secret"), "{text}");
    }

    #[test]
    fn udp_associate_asks_for_any_address() {
        assert_eq!(udp_associate_request(), [5, 3, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(check_associate_head([5, 0, 0, 1]), Ok(Some(6)));
        assert_eq!(check_associate_head([5, 0, 0, 4]), Ok(Some(18)));
        assert_eq!(check_associate_head([5, 0, 0, 3]), Ok(None));
        assert_eq!(check_associate_head([5, 2, 0, 1]), Err(SocksError::NotPermitted));
        assert_eq!(check_associate_head([5, 7, 0, 1]), Err(SocksError::UdpNotGranted));
        assert_eq!(check_associate_head([5, 0, 0, 9]), Err(SocksError::Unknown));
        assert_eq!(check_associate_head([4, 0, 0, 1]), Err(SocksError::Unknown));
    }

    #[test]
    fn the_relay_is_the_bound_address_or_the_proxy() {
        let proxy: SocketAddr = "10.0.0.2:1080".parse().expect("address");
        assert_eq!(
            relay_address(ATYP_IPV4, &[10, 0, 0, 9, 0x13, 0x88], proxy),
            Ok("10.0.0.9:5000".parse().expect("address"))
        );
        assert_eq!(
            relay_address(ATYP_IPV4, &[0, 0, 0, 0, 0x13, 0x88], proxy),
            Ok("10.0.0.2:5000".parse().expect("address"))
        );
        assert_eq!(
            relay_address(ATYP_DOMAIN, &[b'r', b'e', b'l', 0x13, 0x88], proxy),
            Ok("10.0.0.2:5000".parse().expect("address"))
        );
        assert_eq!(
            relay_address(ATYP_IPV4, &[10, 0, 0, 9, 0, 0], proxy),
            Err(SocksError::UdpNotGranted)
        );
        assert_eq!(relay_address(ATYP_IPV4, &[10, 0], proxy), Err(SocksError::Unknown));
    }

    #[test]
    fn packets_wrap_and_unwrap() {
        let sim: SocketAddr = "203.0.113.7:13005".parse().expect("address");
        let mut wire = vec![9; 4];
        wrap(sim, b"packet", &mut wire);
        assert_eq!(&wire[..10], &[0, 0, 0, 1, 203, 0, 113, 7, 0x32, 0xcd]);
        assert_eq!(&wire[10..], b"packet");
        assert_eq!(unwrap(&wire), Some((sim, 10)));

        let sim6: SocketAddr = "[2001:db8::1]:443".parse().expect("address");
        wrap(sim6, b"x", &mut wire);
        assert_eq!(wire.len(), 4 + 16 + 2 + 1);
        assert_eq!(unwrap(&wire), Some((sim6, 22)));
    }

    #[test]
    fn fragments_names_and_short_datagrams_are_dropped() {
        assert_eq!(unwrap(&[0, 0, 1, 1, 1, 2, 3, 4, 0, 80, 7]), None, "fragment");
        assert_eq!(unwrap(&[0, 0, 0, 3, 1, b'a', 0, 80]), None, "name");
        assert_eq!(unwrap(&[0, 0, 0, 1, 1, 2, 3, 4, 0]), None, "short");
        assert_eq!(unwrap(&[0, 0]), None, "short");
        assert_eq!(
            unwrap(&[0, 0, 0, 1, 1, 2, 3, 4, 0, 80]),
            Some(("1.2.3.4:80".parse().expect("address"), 10)),
            "an empty packet still unwraps"
        );
    }

    /// The whole handshake against a scripted proxy, in memory.
    #[tokio::test]
    async fn the_handshake_reads_the_relay() {
        let proxy: SocketAddr = "10.0.0.2:1080".parse().expect("address");
        let (mut viewer, mut server) = tokio::io::duplex(256);
        let script = tokio::spawn(async move {
            let mut greeting = [0u8; 3];
            server.read_exact(&mut greeting).await.expect("greeting");
            assert_eq!(greeting, [5, 1, 2]);
            server.write_all(&[5, 2]).await.expect("method");
            let mut request = [0u8; 9];
            server.read_exact(&mut request).await.expect("credentials");
            assert_eq!(request, [1, 2, b'u', b'1', 4, b'p', b'a', b's', b's']);
            server.write_all(&[1, 0]).await.expect("status");
            let mut associate = [0u8; 10];
            server.read_exact(&mut associate).await.expect("associate");
            assert_eq!(associate, udp_associate_request());
            server.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0x30, 0x39]).await.expect("reply");
        });
        let relay = handshake(&mut viewer, Some(&auth("u1", "pass")), proxy).await;
        script.await.expect("script");
        assert_eq!(relay, Ok("10.0.0.2:12345".parse().expect("address")));
    }

    #[tokio::test]
    async fn a_refused_password_fails_the_handshake() {
        let proxy: SocketAddr = "10.0.0.2:1080".parse().expect("address");
        let (mut viewer, mut server) = tokio::io::duplex(256);
        let script = tokio::spawn(async move {
            let mut buf = [0u8; 3];
            server.read_exact(&mut buf).await.expect("greeting");
            server.write_all(&[5, 2]).await.expect("method");
            let mut request = [0u8; 5];
            server.read_exact(&mut request).await.expect("credentials");
            server.write_all(&[1, 1]).await.expect("status");
        });
        let relay = handshake(&mut viewer, Some(&auth("u", "p")), proxy).await;
        script.await.expect("script");
        assert_eq!(relay, Err(SocksError::AuthFail));
    }

    #[tokio::test]
    async fn no_acceptable_method_is_reported() {
        let proxy: SocketAddr = "10.0.0.2:1080".parse().expect("address");
        let (mut viewer, mut server) = tokio::io::duplex(256);
        let script = tokio::spawn(async move {
            let mut buf = [0u8; 3];
            server.read_exact(&mut buf).await.expect("greeting");
            server.write_all(&[5, 0xff]).await.expect("method");
        });
        let relay = handshake(&mut viewer, None, proxy).await;
        script.await.expect("script");
        assert_eq!(relay, Err(SocksError::NotAcceptable));
    }
}
