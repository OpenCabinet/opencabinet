// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One person's place in a session, whichever way they are in it: hosting
//! it, joined to someone who hosts it, or on a session server; and what
//! happens when the host hands the session over (RFC-0004 §6).
//!
//! Handing over: the host writes in the session who will host it next.
//! That person starts hosting the copy they already have and writes the
//! new invitation in the session. Everyone else, the old host included,
//! connects to the new host with the copy they have, so nothing is lost
//! on the way; a few seconds later the old host stops listening.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::net::{Guest, Host, Link, local_addresses};
use crate::{Shared, server};

/// How long an old host keeps its door open after the session moved.
const LINGER: Duration = Duration::from_secs(5);
/// How long a host leaving waits for someone to take the session over.
const HANDING: Duration = Duration::from_secs(20);
/// How often connecting to a new host is tried again.
const RETRY: Duration = Duration::from_secs(2);

/// Where this participant listens when it hosts.
#[derive(Clone, Debug)]
enum Network {
    /// Every network of this computer.
    Local,
    /// This computer only, for tests.
    Loopback,
}

impl Network {
    fn host(&self, link: Link, pdf: Arc<Vec<u8>>) -> Result<Host, String> {
        match self {
            Self::Local => Host::start_with(
                link,
                pdf,
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                &local_addresses(),
            ),
            Self::Loopback => Host::start_with(
                link,
                pdf,
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                &[Ipv4Addr::LOCALHOST],
            ),
        }
    }
}

/// One person in a session.
#[derive(Debug)]
pub struct Participant {
    link: Link,
    name: String,
    pdf: Arc<Vec<u8>>,
    network: Network,
    /// The session hosted here, if it is.
    host: Option<Host>,
    /// The invitation this participant is connected through, when it is
    /// not the host.
    via: Option<String>,
    /// Whether the session is on a server.
    on_server: bool,
    /// Old hosts closing, and since when.
    closing: Vec<(Host, Instant)>,
    /// When connecting to a new host was last tried.
    tried: Option<Instant>,
}

impl Participant {
    /// Hosts a new session from this computer.
    ///
    /// # Errors
    /// When the computer cannot listen.
    pub fn host(shared: Shared, pdf: Vec<u8>, name: &str) -> Result<Self, String> {
        Self::host_on(shared, pdf, name, Network::Local)
    }

    /// Hosts a new session reachable from this computer only, for tests.
    ///
    /// # Errors
    /// When the computer cannot listen.
    pub fn host_here(shared: Shared, pdf: Vec<u8>, name: &str) -> Result<Self, String> {
        Self::host_on(shared, pdf, name, Network::Loopback)
    }

    fn host_on(shared: Shared, pdf: Vec<u8>, name: &str, network: Network) -> Result<Self, String> {
        let pdf = Arc::new(pdf);
        let host = network.host(Link::new(shared), Arc::clone(&pdf))?;
        Ok(Self {
            link: host.link.clone(),
            name: name.to_owned(),
            pdf,
            network,
            host: Some(host),
            via: None,
            on_server: false,
            closing: Vec::new(),
            tried: None,
        })
    }

    /// Joins the session `invite` names; this participant and the PDF.
    ///
    /// # Errors
    /// A message for the user when it cannot.
    pub fn join(invite: &str, name: &str) -> Result<(Self, Vec<u8>), String> {
        let guest = Guest::join(invite, name)?;
        let pdf = guest.pdf.clone();
        let mut joined = Self {
            link: guest.link,
            name: name.to_owned(),
            pdf: Arc::new(guest.pdf),
            network: Network::Local,
            host: None,
            via: Some(invite.trim().to_owned()),
            on_server: false,
            closing: Vec::new(),
            tried: None,
        };
        joined.on_server = invite
            .trim()
            .split(':')
            .nth(1)
            .is_some_and(|a| a.split(',').any(|a| a.parse::<Ipv4Addr>().is_err()));
        Ok((joined, pdf))
    }

    /// As [`Participant::join`], hosting on this computer only if the
    /// session is handed over to it, for tests.
    ///
    /// # Errors
    /// As [`Participant::join`].
    pub fn join_here(invite: &str, name: &str) -> Result<(Self, Vec<u8>), String> {
        let (mut joined, pdf) = Self::join(invite, name)?;
        joined.network = Network::Loopback;
        Ok((joined, pdf))
    }

    /// Starts a session on the session server at `server` (`ocs1:…`).
    ///
    /// # Errors
    /// A message for the user when it cannot.
    pub fn start_on_server(
        server: &str,
        shared: Shared,
        pdf: Vec<u8>,
        name: &str,
    ) -> Result<Self, String> {
        let (link, invite) = server::start_on_server(server, shared, &pdf)?;
        Ok(Self {
            link,
            name: name.to_owned(),
            pdf: Arc::new(pdf),
            network: Network::Local,
            host: None,
            via: Some(invite.to_string()),
            on_server: true,
            closing: Vec::new(),
            tried: None,
        })
    }

    /// This participant's copy and its connections.
    #[must_use]
    pub fn link(&self) -> &Link {
        &self.link
    }

    /// The invitation to send others: this computer's when hosting, the
    /// server's when the session is on one.
    #[must_use]
    pub fn invite(&self) -> Option<String> {
        if let Some(host) = &self.host {
            return Some(host.invite.to_string());
        }
        self.via.clone().filter(|_| self.on_server)
    }

    /// Whether the session is hosted from this computer.
    #[must_use]
    pub fn hosting(&self) -> bool {
        self.host.is_some()
    }

    /// Whether the session is on a session server.
    #[must_use]
    pub fn on_server(&self) -> bool {
        self.on_server
    }

    /// The others in the session, by name.
    #[must_use]
    pub fn others(&self) -> Vec<String> {
        self.link
            .shared
            .lock()
            .map_or_else(|_| Vec::new(), |s| s.others())
    }

    /// Hands the session over to `name`, when hosting it.
    ///
    /// # Errors
    /// When this computer does not host the session, or nobody else is
    /// called `name`.
    pub fn hand_over(&mut self, name: &str) -> Result<(), String> {
        if self.host.is_none() {
            return Err("Only the host can hand the session over".into());
        }
        let handed = self
            .link
            .shared
            .lock()
            .map_err(|_| "The session is gone")?
            .hand_over(name);
        if handed {
            Ok(())
        } else {
            Err(format!("{name} is not in the session"))
        }
    }

    /// Does what the session asks of this participant now: hosting it when
    /// it was handed over here, connecting to its new host when it moved,
    /// closing an old door. Says what changed, if anything did.
    pub fn follow(&mut self) -> Option<String> {
        if self.link.stopped() {
            return None;
        }
        self.closing.retain(|(host, since)| {
            let done = since.elapsed() >= LINGER;
            if done {
                host.close();
            }
            !done
        });
        let (mine, moved) = {
            let shared = self.link.shared.lock().ok()?;
            (shared.handed_to_me(), shared.moved_to())
        };
        if mine && self.host.is_none() {
            return Some(
                match self.network.host(self.link.clone(), Arc::clone(&self.pdf)) {
                    Ok(host) => {
                        let invite = host.invite.to_string();
                        if let Ok(mut shared) = self.link.shared.lock() {
                            shared.moved_here(&invite);
                        }
                        self.host = Some(host);
                        "You host the session now. The others are moving to your computer.".into()
                    }
                    Err(e) => {
                        format!("The session was handed to you, but cannot be hosted here: {e}")
                    }
                },
            );
        }
        let moved = moved?;
        let here = self.host.as_ref().map(|h| h.invite.to_string());
        if here.as_ref() == Some(&moved) || self.via.as_ref() == Some(&moved) {
            return None;
        }
        if self.tried.is_some_and(|t| t.elapsed() < RETRY) {
            return None;
        }
        self.tried = Some(Instant::now());
        match Guest::rejoin(&moved, &self.link, &self.name) {
            Ok(()) => {
                self.via = Some(moved);
                self.on_server = false;
                let was_host = self.host.take();
                if let Some(old) = was_host {
                    self.closing.push((old, Instant::now()));
                    Some("The session moved to its new host; you are in it as a guest.".into())
                } else {
                    Some("The session moved to its new host.".into())
                }
            }
            Err(_) => None,
        }
    }

    /// Leaves the session. A host leaving hands it to someone else first,
    /// when anyone is left, and keeps the door open until they host it.
    pub fn leave(self) {
        let Some(host) = self.host else {
            self.link.stop();
            for (old, _) in self.closing {
                old.close();
            }
            return;
        };
        let next = self
            .link
            .shared
            .lock()
            .ok()
            .and_then(|s| s.others().into_iter().next());
        let handed = next.is_some_and(|name| {
            self.link
                .shared
                .lock()
                .is_ok_and(|mut s| s.hand_over(&name))
        });
        if !handed {
            self.link.stop();
            host.close();
            return;
        }
        let link = self.link;
        thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < HANDING
                && link.shared.lock().ok().and_then(|s| s.moved_to()).is_none()
            {
                thread::sleep(Duration::from_millis(100));
            }
            thread::sleep(LINGER);
            link.stop();
            host.close();
        });
    }
}
