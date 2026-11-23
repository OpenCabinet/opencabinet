// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sessions held by a server instead of a desktop (RFC-0004 §7), so people
//! in different offices can review together.
//!
//! A server keeps rooms. Whoever starts a session sends the server the
//! PDF and the session as they are, and gets back the room's secret: the
//! invitation is then an ordinary one, with the server's address and the
//! fingerprint of its certificate, and whoever joins does so exactly as
//! with a desktop host. The server holds a copy of each room only to pass
//! changes on; it is not a person in the session.
//!
//! The server's certificate is kept on its disk, so its fingerprint, and
//! the server address people type, stay the same between runs.

use std::collections::HashMap;
use std::fmt;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::net::{
    CREATE, CREATED, Frames, HELLO, WELCOME, connect, hex, is_address, keep_in_step, provider,
    stay, welcome_bytes, welcome_parts, write_frame,
};
use crate::{Invite, Link, Shared};

/// How long a room nobody is in is kept.
const EMPTY_ROOM: Duration = Duration::from_hours(1);
/// The most rooms a server keeps at once.
const MAX_ROOMS: usize = 1000;
/// How long a connection may take to say what it wants.
const GREETING: Duration = Duration::from_secs(30);
/// How often the server looks for new connections and old rooms.
const TICK: Duration = Duration::from_millis(40);

/// Where a session server is: written `ocs1:<address>:<port>:<fingerprint>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerAddress {
    /// Its name or IPv4 address.
    pub address: String,
    /// The port it listens on.
    pub port: u16,
    /// The SHA-256 of its certificate.
    pub fingerprint: [u8; 32],
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = hex(&self.fingerprint);
        write!(f, "ocs1:{}:{}:{hex}", self.address, self.port)
    }
}

impl FromStr for ServerAddress {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        let bad = || "This is not a session server's address (ocs1:…)".to_owned();
        let parts: Vec<&str> = text.trim().split(':').collect();
        let ["ocs1", address, port, hex] = parts[..] else {
            return Err(bad());
        };
        let port: u16 = port.parse().map_err(|_| bad())?;
        if !is_address(address) || port == 0 || hex.len() != 64 {
            return Err(bad());
        }
        let mut fingerprint = [0; 32];
        for (i, byte) in fingerprint.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2).ok_or_else(bad)?, 16)
                .map_err(|_| bad())?;
        }
        Ok(Self {
            address: address.to_owned(),
            port,
            fingerprint,
        })
    }
}

/// A server's certificate and key.
#[derive(Clone, Debug)]
pub struct Identity {
    certificate: Vec<u8>,
    key: Vec<u8>,
}

impl Identity {
    /// A new certificate and key.
    ///
    /// # Errors
    /// When they cannot be made.
    pub fn new() -> Result<Self, String> {
        let certified = rcgen::generate_simple_self_signed(vec!["opencabinet-review".to_owned()])
            .map_err(|e| format!("Cannot make the server's certificate: {e}"))?;
        Ok(Self {
            certificate: certified.cert.der().to_vec(),
            key: certified.signing_key.serialize_der(),
        })
    }

    /// The certificate and key kept in `folder`, made and kept there the
    /// first time.
    ///
    /// # Errors
    /// When the folder cannot be read or written.
    pub fn kept_in(folder: &Path) -> Result<Self, String> {
        let (certificate, key) = (folder.join("certificate.der"), folder.join("key.der"));
        if let (Ok(certificate), Ok(key)) = (std::fs::read(&certificate), std::fs::read(&key)) {
            return Ok(Self { certificate, key });
        }
        let made = Self::new()?;
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        std::fs::write(&key, &made.key).map_err(|e| e.to_string())?;
        std::fs::write(&certificate, &made.certificate).map_err(|e| e.to_string())?;
        Ok(made)
    }

    /// The SHA-256 of the certificate.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut out = [0; 32];
        out.copy_from_slice(digest(&SHA256, &self.certificate).as_ref());
        out
    }
}

/// A session kept by the server.
struct Room {
    link: Link,
    pdf: Arc<Vec<u8>>,
    /// How many are in it, and since when it is empty.
    present: usize,
    empty_since: Instant,
}

type Rooms = Arc<Mutex<HashMap<[u8; 16], Room>>>;

/// A running session server.
#[derive(Debug)]
pub struct Server {
    /// Where people reach it.
    pub address: ServerAddress,
    stop: Arc<AtomicBool>,
    rooms: Arc<Mutex<HashMap<[u8; 16], Room>>>,
}

impl fmt::Debug for Room {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Room")
            .field("present", &self.present)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Listens on `bind` with `identity`, telling people to come to
    /// `address`, a name or IPv4 address others reach it at.
    ///
    /// # Errors
    /// When it cannot listen.
    pub fn start(bind: SocketAddr, address: &str, identity: &Identity) -> Result<Self, String> {
        if !is_address(address) {
            return Err(format!("{address} is not a name or an IPv4 address"));
        }
        let config = ServerConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(identity.certificate.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone())),
            )
            .map_err(|e| e.to_string())?;
        let config = Arc::new(config);
        let listener = TcpListener::bind(bind).map_err(|e| format!("Cannot listen: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let rooms: Rooms = Arc::default();
        let (stopping, kept) = (Arc::clone(&stop), Arc::clone(&rooms));
        thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (config, rooms, stop) = (
                            Arc::clone(&config),
                            Arc::clone(&kept),
                            Arc::clone(&stopping),
                        );
                        thread::spawn(move || {
                            let _ = serve(stream, &config, &rooms, &stop);
                        });
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        forget_empty(&kept);
                        thread::sleep(TICK);
                    }
                    Err(_) => thread::sleep(TICK),
                }
            }
        });
        Ok(Self {
            address: ServerAddress {
                address: address.to_owned(),
                port,
                fingerprint: identity.fingerprint(),
            },
            stop,
            rooms,
        })
    }

    /// How many rooms it keeps.
    #[must_use]
    pub fn rooms(&self) -> usize {
        self.rooms.lock().map_or(0, |r| r.len())
    }

    /// Stops listening and closes every room.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(rooms) = self.rooms.lock() {
            for room in rooms.values() {
                room.link.stop();
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Drops the rooms nobody has been in for a while.
fn forget_empty(rooms: &Rooms) {
    if let Ok(mut rooms) = rooms.lock() {
        rooms.retain(|_, room| {
            let keep = room.present > 0 || room.empty_since.elapsed() < EMPTY_ROOM;
            if !keep {
                room.link.stop();
            }
            keep
        });
    }
}

/// Lets a connection into its room, or makes it a new one, then keeps it
/// in step.
fn serve(
    stream: TcpStream,
    config: &Arc<ServerConfig>,
    rooms: &Rooms,
    stop: &AtomicBool,
) -> Result<(), String> {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(GREETING))
        .map_err(|e| e.to_string())?;
    let connection = ServerConnection::new(Arc::clone(config)).map_err(|e| e.to_string())?;
    let mut tls = StreamOwned::new(connection, stream);
    let mut frames = Frames::default();
    let (kind, body) = frames.read_blocking(&mut tls)?;
    let (secret, link) = match kind {
        HELLO => {
            let secret: [u8; 16] = body
                .get(..16)
                .and_then(|s| s.try_into().ok())
                .ok_or("not invited")?;
            let (link, pdf) = {
                let mut rooms = rooms.lock().map_err(|_| "the server stopped")?;
                let room = rooms.get_mut(&secret).ok_or("not invited")?;
                room.present += 1;
                (room.link.clone(), Arc::clone(&room.pdf))
            };
            let session = link.shared.lock().map_err(|_| "the room is gone")?.save();
            let welcome = welcome_bytes(&pdf, &session);
            match welcome.and_then(|w| write_frame(&mut tls, WELCOME, &w)) {
                Ok(()) => (secret, link),
                Err(e) => {
                    leave(rooms, secret);
                    return Err(e);
                }
            }
        }
        CREATE => {
            let (pdf, session) = welcome_parts(&body)?;
            let shared = Shared::relay(session)?;
            let mut secret = [0; 16];
            SystemRandom::new()
                .fill(&mut secret)
                .map_err(|_| "no randomness")?;
            let link = Link::new(shared);
            {
                let mut rooms = rooms.lock().map_err(|_| "the server stopped")?;
                if rooms.len() >= MAX_ROOMS {
                    return Err("the server is full".into());
                }
                rooms.insert(
                    secret,
                    Room {
                        link: link.clone(),
                        pdf: Arc::new(pdf.to_vec()),
                        present: 1,
                        empty_since: Instant::now(),
                    },
                );
            }
            if let Err(e) = write_frame(&mut tls, CREATED, &secret) {
                leave(rooms, secret);
                return Err(e);
            }
            (secret, link)
        }
        _ => return Err("not a review session".into()),
    };
    let result = keep_in_step(tls, frames, &link, stop);
    leave(rooms, secret);
    result
}

/// One fewer is in room `secret`.
fn leave(rooms: &Rooms, secret: [u8; 16]) {
    if let Ok(mut rooms) = rooms.lock()
        && let Some(room) = rooms.get_mut(&secret)
    {
        room.present = room.present.saturating_sub(1);
        if room.present == 0 {
            room.empty_since = Instant::now();
        }
    }
}

/// Starts a session on the server at `server` with `shared` and `pdf`;
/// this participant's copy and the invitation to send.
///
/// # Errors
/// A message for the user when the address is wrong, the server cannot be
/// reached or is not who the address says, or it refuses.
pub fn start_on_server(server: &str, shared: Shared, pdf: &[u8]) -> Result<(Link, Invite), String> {
    let server: ServerAddress = server.parse()?;
    let (mut tls, _) = connect(
        std::slice::from_ref(&server.address),
        server.port,
        server.fingerprint,
    )
    .map_err(|e| e.replace("the host", "the server"))?;
    let link = Link::new(shared);
    let session = link
        .shared
        .lock()
        .map_err(|_| "the session is gone")?
        .save();
    write_frame(&mut tls, CREATE, &welcome_bytes(pdf, &session)?)?;
    let mut frames = Frames::default();
    let (kind, body) = frames
        .read_blocking(&mut tls)
        .map_err(|e| format!("The server did not start the session: {e}"))?;
    let secret: [u8; 16] = body
        .get(..16)
        .and_then(|s| s.try_into().ok())
        .filter(|_| kind == CREATED)
        .ok_or("The server did not start the session")?;
    let invite = Invite {
        addresses: vec![server.address],
        port: server.port,
        secret,
        fingerprint: server.fingerprint,
    };
    stay(tls, frames, link.clone(), invite.to_string());
    Ok((link, invite))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_address_is_written_and_read_back() {
        let address = ServerAddress {
            address: "review.example.org".into(),
            port: 47100,
            fingerprint: [0xcd; 32],
        };
        let text = address.to_string();
        assert!(text.starts_with("ocs1:review.example.org:47100:cdcd"));
        assert_eq!(text.parse::<ServerAddress>().unwrap(), address);
        assert!("ocr1:a:1:00".parse::<ServerAddress>().is_err());
        assert!(
            text.replacen(":47100:", ":0:", 1)
                .parse::<ServerAddress>()
                .is_err()
        );
    }

    #[test]
    fn an_identity_is_kept_between_runs() {
        let folder = std::env::temp_dir().join(format!("oc-server-{}", std::process::id()));
        let first = Identity::kept_in(&folder).unwrap();
        let again = Identity::kept_in(&folder).unwrap();
        assert_eq!(first.fingerprint(), again.fingerprint());
        std::fs::remove_dir_all(&folder).ok();
    }
}
