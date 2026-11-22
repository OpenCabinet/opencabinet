// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hosting a session from the desktop and joining one (RFC-0004 §6, §7).
//!
//! The host listens on its network and makes a certificate of its own for
//! the session. The invitation carries the host's address, a secret and
//! the certificate's fingerprint: whoever joins checks the host's
//! certificate against that fingerprint, so the connection is encrypted
//! with TLS and no certificate authority is needed, and the host admits
//! only who knows the secret.
//!
//! On the connection, frames carry a kind and a length. Whoever joins says
//! hello with the secret and their name; the host answers with the PDF
//! and the session as it is; from then on both send Automerge sync
//! messages whenever they have something to say.

use std::fmt;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use automerge::sync::State;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
    SignatureScheme, StreamOwned,
};

use crate::Shared;

/// The name in the session's certificate.
const NAME: &str = "opencabinet-review";
/// The largest frame accepted: a PDF of drawings may be large.
const MAX_FRAME: u32 = 512 * 1024 * 1024;
/// How long a read waits before the loop looks for something to send.
const TICK: Duration = Duration::from_millis(40);
/// The port a host listens on when it is free, so that a firewall rule
/// or a router can name it; another one otherwise.
pub const PORT: u16 = 47100;
/// How long whoever joins waits for the host to answer at an address.
const KNOCK: Duration = Duration::from_secs(6);
/// How long joining may take before either side gives up.
const GREETING: Duration = Duration::from_secs(30);

const HELLO: u8 = 1;
const WELCOME: u8 = 2;
const SYNC: u8 = 3;

/// What someone needs to join a session.
///
/// Written `ocr1:<addresses>:<port>:<secret>:<fingerprint>`, the
/// addresses separated by commas: a computer may be on more than one
/// network, and whoever joins tries them all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// The host's IPv4 addresses, the likeliest first.
    pub addresses: Vec<Ipv4Addr>,
    /// The port the host listens on.
    pub port: u16,
    /// What proves the invitation.
    pub secret: [u8; 16],
    /// The SHA-256 of the host's certificate.
    pub fingerprint: [u8; 32],
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut out = [0; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

impl fmt::Display for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let addresses: Vec<String> = self.addresses.iter().map(ToString::to_string).collect();
        write!(
            f,
            "ocr1:{}:{}:{}:{}",
            addresses.join(","),
            self.port,
            hex(&self.secret),
            hex(&self.fingerprint)
        )
    }
}

impl FromStr for Invite {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || "This is not an invitation to a review session".to_owned();
        let parts: Vec<&str> = text.trim().split(':').collect();
        let [tag, addresses, port, secret, fingerprint] = parts[..] else {
            return Err(bad());
        };
        if tag != "ocr1" {
            return Err(bad());
        }
        let addresses = addresses
            .split(',')
            .map(|a| a.trim().parse::<Ipv4Addr>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| bad())?;
        let port: u16 = port.parse().map_err(|_| bad())?;
        if addresses.is_empty() || port == 0 {
            return Err(bad());
        }
        Ok(Self {
            addresses,
            port,
            secret: unhex(secret).ok_or_else(bad)?,
            fingerprint: unhex(fingerprint).ok_or_else(bad)?,
        })
    }
}

/// What a participant shares with the connections that keep it in step.
#[derive(Clone, Debug)]
pub struct Link {
    /// The participant's copy of the session.
    pub shared: Arc<Mutex<Shared>>,
    /// Bumped whenever someone else's change arrives.
    pub changed: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl Link {
    fn new(shared: Shared) -> Self {
        Self {
            shared: Arc::new(Mutex::new(shared)),
            changed: Arc::new(AtomicU64::new(0)),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Ends every connection of this participant.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Whether the session has ended here.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The addresses this computer is reached at on its networks, the one it
/// reaches the internet through first; the loopback address when it has
/// none.
#[must_use]
pub fn local_addresses() -> Vec<Ipv4Addr> {
    // Connecting a UDP socket sends nothing; it only picks the address.
    let routed = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            socket.local_addr()
        })
        .ok()
        .and_then(|a| match a.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        });
    // The computer's own name resolves to the addresses of every network
    // it is on.
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    let named = (name.as_str(), 0)
        .to_socket_addrs()
        .into_iter()
        .flatten()
        .filter_map(|a| match a.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        });
    let mut addresses: Vec<Ipv4Addr> = Vec::new();
    for ip in routed.into_iter().chain(named) {
        let usable = !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified();
        if usable && !addresses.contains(&ip) {
            addresses.push(ip);
        }
    }
    if addresses.is_empty() {
        addresses.push(Ipv4Addr::LOCALHOST);
    }
    addresses
}

/// A session hosted from this computer.
#[derive(Debug)]
pub struct Host {
    /// The invitation to send to who joins.
    pub invite: Invite,
    /// This participant's copy and its connections.
    pub link: Link,
}

impl Host {
    /// Hosts `shared`, with `pdf` the drawing everyone reviews, on this
    /// computer's network.
    ///
    /// # Errors
    /// When the computer cannot listen or make its certificate.
    pub fn start(shared: Shared, pdf: Arc<Vec<u8>>) -> Result<Self, String> {
        Self::start_on(
            shared,
            pdf,
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            local_addresses(),
        )
    }

    /// Hosts on `bind`, telling who joins to come to one of `shown`.
    ///
    /// # Errors
    /// When the computer cannot listen or make its certificate.
    pub fn start_on(
        shared: Shared,
        pdf: Arc<Vec<u8>>,
        bind: IpAddr,
        shown: Vec<Ipv4Addr>,
    ) -> Result<Self, String> {
        let certified = rcgen::generate_simple_self_signed(vec![NAME.to_owned()])
            .map_err(|e| format!("Cannot make the session's certificate: {e}"))?;
        let certificate = certified.cert.der().clone();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            certified.signing_key.serialize_der(),
        ));
        let mut fingerprint = [0; 32];
        fingerprint.copy_from_slice(digest(&SHA256, &certificate).as_ref());
        let mut secret = [0; 16];
        SystemRandom::new()
            .fill(&mut secret)
            .map_err(|_| "Cannot make the invitation's secret".to_owned())?;
        let config = ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_single_cert(vec![certificate], key)
            .map_err(|e| e.to_string())?;
        let config = Arc::new(config);
        let listener = TcpListener::bind((bind, PORT))
            .or_else(|_| TcpListener::bind((bind, 0)))
            .map_err(|e| format!("Cannot listen: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let link = Link::new(shared);
        let accepting = link.clone();
        thread::spawn(move || {
            while !accepting.stopped() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let link = accepting.clone();
                        let config = Arc::clone(&config);
                        let pdf = Arc::clone(&pdf);
                        thread::spawn(move || {
                            let _ = admit(stream, &config, &pdf, secret, &link);
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(TICK),
                    Err(_) => thread::sleep(TICK),
                }
            }
        });
        Ok(Self {
            invite: Invite {
                addresses: shown,
                port,
                secret,
                fingerprint,
            },
            link,
        })
    }
}

/// Lets someone in when they know the secret, then keeps them in step.
fn admit(
    stream: TcpStream,
    config: &Arc<ServerConfig>,
    pdf: &[u8],
    secret: [u8; 16],
    link: &Link,
) -> Result<(), String> {
    // On Windows a connection accepted from a listener that does not
    // block does not block either: sending the PDF across a network would
    // stop at the first full buffer and drop who is joining.
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_nodelay(true).ok();
    // Who does not say hello in time is not waited for.
    stream
        .set_read_timeout(Some(GREETING))
        .map_err(|e| e.to_string())?;
    let connection = ServerConnection::new(Arc::clone(config)).map_err(|e| e.to_string())?;
    let mut tls = StreamOwned::new(connection, stream);
    let mut frames = Frames::default();
    let (kind, hello) = frames.read_blocking(&mut tls)?;
    if kind != HELLO || hello.len() < 16 || hello[..16] != secret {
        return Err("not invited".into());
    }
    let session = link
        .shared
        .lock()
        .map_err(|_| "the session is gone")?
        .save();
    let mut welcome = u32::try_from(pdf.len())
        .map_err(|_| "the PDF is too large")?
        .to_be_bytes()
        .to_vec();
    welcome.extend_from_slice(pdf);
    welcome.extend_from_slice(&session);
    write_frame(&mut tls, WELCOME, &welcome)?;
    keep_in_step(tls, frames, link)
}

/// A session joined from this computer.
#[derive(Debug)]
pub struct Guest {
    /// This participant's copy and its connection.
    pub link: Link,
    /// The drawing everyone reviews, as the host sent it.
    pub pdf: Vec<u8>,
}

impl Guest {
    /// Joins the session `invite` names, as `name`.
    ///
    /// # Errors
    /// A message for the user when the invitation is wrong, the host
    /// cannot be reached, or the host is not who the invitation says.
    pub fn join(invite: &str, name: &str) -> Result<Self, String> {
        let invite: Invite = invite.parse()?;
        let config = ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pinned {
                fingerprint: invite.fingerprint,
                provider: provider(),
            }))
            .with_no_client_auth();
        let (stream, address) = knock(&invite)?;
        stream.set_nodelay(true).ok();
        stream
            .set_read_timeout(Some(GREETING))
            .map_err(|e| e.to_string())?;
        let server = ServerName::try_from(NAME).map_err(|e| e.to_string())?;
        let connection =
            ClientConnection::new(Arc::new(config), server).map_err(|e| e.to_string())?;
        let mut tls = StreamOwned::new(connection, stream);
        let mut hello = invite.secret.to_vec();
        hello.extend_from_slice(name.as_bytes());
        write_frame(&mut tls, HELLO, &hello)
            .map_err(|e| format!("The host did not answer as expected: {e}"))?;
        let mut frames = Frames::default();
        let (kind, welcome) = frames.read_blocking(&mut tls).map_err(|e| {
            format!("The host at {address} closed the connection while letting you in: {e}")
        })?;
        if kind != WELCOME || welcome.len() < 4 {
            return Err("The host did not let you in".into());
        }
        let length = u32::from_be_bytes([welcome[0], welcome[1], welcome[2], welcome[3]]);
        let length = usize::try_from(length).map_err(|e| e.to_string())?;
        let pdf = welcome
            .get(4..4 + length)
            .ok_or("The host sent a broken drawing")?
            .to_vec();
        let shared = Shared::join(&welcome[4 + length..], name)?;
        let link = Link::new(shared);
        let running = link.clone();
        thread::spawn(move || {
            let _ = keep_in_step(tls, frames, &running);
            running.stop();
        });
        Ok(Self { link, pdf })
    }
}

/// Tries every address of the invitation at once, and keeps the first
/// that answers.
fn knock(invite: &Invite) -> Result<(TcpStream, SocketAddr), String> {
    let (answered, answers) = mpsc::channel();
    for &ip in &invite.addresses {
        let address = SocketAddr::new(IpAddr::V4(ip), invite.port);
        let answered = answered.clone();
        thread::spawn(move || {
            let _ = answered.send((address, TcpStream::connect_timeout(&address, KNOCK)));
        });
    }
    drop(answered);
    let mut failures = Vec::new();
    for (address, result) in answers {
        match result {
            Ok(stream) => return Ok((stream, address)),
            Err(e) => failures.push(format!("{address}: {e}")),
        }
    }
    Err(format!(
        "Cannot reach the host ({}). Check that both computers are on the same network, and that the host let OpenCabinet Review through the Windows firewall (Session panel, on the host).",
        failures.join("; ")
    ))
}

/// Accepts the host's certificate only when it is the one the invitation
/// names.
#[derive(Debug)]
struct Pinned {
    fingerprint: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if digest(&SHA256, end_entity).as_ref() == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "the host is not the one in the invitation".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Frames read so far from a connection, and the bytes of the next one.
#[derive(Default)]
struct Frames {
    buffer: Vec<u8>,
}

impl Frames {
    /// The next whole frame in the buffer, if one has arrived.
    fn next(&mut self) -> Result<Option<(u8, Vec<u8>)>, String> {
        if self.buffer.len() < 5 {
            return Ok(None);
        }
        let length = u32::from_be_bytes([
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
            self.buffer[4],
        ]);
        if length > MAX_FRAME {
            return Err("a frame is too large".into());
        }
        let end = 5 + usize::try_from(length).map_err(|e| e.to_string())?;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let kind = self.buffer[0];
        let body = self.buffer[5..end].to_vec();
        self.buffer.drain(..end);
        Ok(Some((kind, body)))
    }

    /// Reads what has arrived, waiting at most the socket's timeout; false
    /// when the connection closed.
    fn fill(&mut self, stream: &mut impl Read) -> Result<bool, String> {
        let mut chunk = [0; 16 * 1024];
        match stream.read(&mut chunk) {
            Ok(0) => Ok(false),
            Ok(n) => {
                self.buffer.extend_from_slice(&chunk[..n]);
                Ok(true)
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(true),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Waits for the next whole frame, as long as data keeps arriving.
    fn read_blocking(&mut self, stream: &mut impl Read) -> Result<(u8, Vec<u8>), String> {
        let mut chunk = vec![0; 64 * 1024];
        loop {
            if let Some(frame) = self.next()? {
                return Ok(frame);
            }
            match stream.read(&mut chunk) {
                Ok(0) => return Err("the connection closed".into()),
                Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    return Err("the other side stopped answering".into());
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

fn write_frame(stream: &mut impl Write, kind: u8, body: &[u8]) -> Result<(), String> {
    let length = u32::try_from(body.len()).map_err(|e| e.to_string())?;
    let mut frame = Vec::with_capacity(body.len() + 5);
    frame.push(kind);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(body);
    stream.write_all(&frame).map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

/// Sends what this side has to say and takes in what the other says,
/// until either side leaves.
fn keep_in_step<C, S>(
    mut tls: StreamOwned<C, TcpStream>,
    mut frames: Frames,
    link: &Link,
) -> Result<(), String>
where
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<S>>,
    S: rustls::SideData,
{
    tls.sock
        .set_read_timeout(Some(TICK))
        .map_err(|e| e.to_string())?;
    let mut state = State::new();
    while !link.stopped() {
        let outgoing = link
            .shared
            .lock()
            .map_err(|_| "the session is gone")?
            .outgoing(&mut state);
        if let Some(message) = outgoing {
            write_frame(&mut tls, SYNC, &message)?;
        }
        if !frames.fill(&mut tls)? {
            return Ok(());
        }
        while let Some((kind, body)) = frames.next()? {
            if kind != SYNC {
                continue;
            }
            let changed = link
                .shared
                .lock()
                .map_err(|_| "the session is gone")?
                .incoming(&mut state, &body)?;
            if changed {
                link.changed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    // Say goodbye properly, so the other side knows this was on purpose.
    tls.conn.send_close_notify();
    let _ = tls.conn.complete_io(&mut tls.sock);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invitation_is_written_and_read_back() {
        let invite = Invite {
            addresses: vec![Ipv4Addr::new(192, 168, 1, 20), Ipv4Addr::new(10, 0, 0, 5)],
            port: 52000,
            secret: [7; 16],
            fingerprint: [0xab; 32],
        };
        let text = invite.to_string();
        assert!(text.starts_with("ocr1:192.168.1.20,10.0.0.5:52000:0707"));
        assert_eq!(text.parse::<Invite>().unwrap(), invite);
        // One address, as the first invitations were written.
        let one = text.replacen(",10.0.0.5", "", 1);
        assert_eq!(one.parse::<Invite>().unwrap().addresses.len(), 1);
        assert!("hello".parse::<Invite>().is_err());
        assert!("ocr1:1.2.3.4:1:00:00".parse::<Invite>().is_err());
        assert!(
            text.replacen(":52000:", ":0:", 1)
                .parse::<Invite>()
                .is_err()
        );
    }

    #[test]
    fn this_computer_has_an_address() {
        let addresses = local_addresses();
        assert_ne!(addresses.len(), 0);
        assert!(addresses.iter().all(|a| !a.is_link_local()));
    }
}
