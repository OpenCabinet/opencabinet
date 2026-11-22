// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A session on this computer: a host and two guests over TLS, marking up
//! at once.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use opencabinet_pdf::{Point, sample};
use opencabinet_review::{Kind, Markup};
use opencabinet_session::{Guest, Host, Invite, Link, Shared};

fn cloud(id: &str, author: &str) -> Markup {
    Markup {
        id: id.into(),
        kind: Kind::Cloud,
        points: vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 5.0),
        ],
        author: author.into(),
        ..Markup::default()
    }
}

fn publish(link: &Link, markups: &[Markup]) {
    link.shared
        .lock()
        .unwrap()
        .publish(markups, &BTreeMap::new());
}

/// Waits until every link holds `count` markups.
fn converge(links: &[&Link], count: usize) {
    let start = Instant::now();
    while links
        .iter()
        .any(|l| l.shared.lock().unwrap().markups().len() != count)
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the session did not converge"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn host() -> Host {
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let pdf = Arc::new(sample::sheets(1, 2592.0, 1728.0, 2));
    Host::start_on(Shared::start("Andrés"), pdf, loopback, loopback).unwrap()
}

#[test]
fn everyone_sees_everyone_s_markups() {
    let host = host();
    publish(&host.link, &[cloud("a", "Andrés")]);
    let invite = host.invite.to_string();
    let maria = Guest::join(&invite, "María").unwrap();
    let juan = Guest::join(&invite, "Juan").unwrap();
    // They received the drawing and what was there.
    assert_eq!(maria.pdf, sample::sheets(1, 2592.0, 1728.0, 2));
    assert_eq!(maria.link.shared.lock().unwrap().markups().len(), 1);

    // Each adds one at the same time.
    let mut theirs = maria.link.shared.lock().unwrap().markups();
    theirs.push(cloud("b", "María"));
    publish(&maria.link, &theirs);
    let mut his = juan.link.shared.lock().unwrap().markups();
    his.push(cloud("c", "Juan"));
    publish(&juan.link, &his);

    converge(&[&host.link, &maria.link, &juan.link], 3);
    assert!(host.link.changed.load(Ordering::SeqCst) > 0);
    let people = host.link.shared.lock().unwrap().people();
    assert_eq!(people, ["Andrés", "Juan", "María"]);
    let activity = host.link.shared.lock().unwrap().activity();
    assert!(
        activity
            .iter()
            .any(|a| a.who == "Juan" && a.what == "added a cloud on page 1")
    );

    // A delete reaches everyone too.
    let mut all = host.link.shared.lock().unwrap().markups();
    all.retain(|m| m.id != "a");
    publish(&host.link, &all);
    converge(&[&host.link, &maria.link, &juan.link], 2);
    host.link.stop();
}

#[test]
fn a_wrong_secret_or_host_is_refused() {
    let host = host();
    let mut wrong = host.invite.clone();
    wrong.secret[0] ^= 1;
    assert!(Guest::join(&wrong.to_string(), "Eve").is_err());
    // A host whose certificate is not the invitation's is not trusted.
    let mut other = host.invite.clone();
    other.fingerprint[0] ^= 1;
    assert!(Guest::join(&other.to_string(), "Eve").is_err());
    assert!(Guest::join("not an invitation", "Eve").is_err());
    let _: Invite = host.invite.to_string().parse().unwrap();
    host.link.stop();
}
