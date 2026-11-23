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
use opencabinet_session::{Guest, Host, Invite, Link, Participant, Shared};

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
    Host::start_on(
        Shared::start("Andrés"),
        pdf,
        loopback,
        &[Ipv4Addr::LOCALHOST],
    )
    .unwrap()
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

#[test]
fn a_large_drawing_reaches_who_joins() {
    // Over a real network the host cannot send a large PDF in one go; it
    // must wait for the guest to take it in rather than give up.
    let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let pdf: Vec<u8> = (0..48 * 1024 * 1024)
        .map(|i: u32| i.to_le_bytes()[0] ^ (i >> 9).to_le_bytes()[0])
        .collect();
    let pdf = Arc::new(pdf);
    let host = Host::start_on(
        Shared::start("Andrés"),
        Arc::clone(&pdf),
        loopback,
        &[Ipv4Addr::LOCALHOST],
    )
    .unwrap();
    let guest = Guest::join(&host.invite.to_string(), "María").unwrap();
    assert_eq!(guest.pdf.len(), pdf.len());
    // Not assert_eq: a failure would print 48 MB.
    assert!(
        guest.pdf.iter().eq(pdf.iter()),
        "the drawing arrived changed"
    );
    host.link.stop();
}

#[test]
fn other_programs_markups_come_back_unchanged() {
    // Markups read from a PDF must come back from the session exactly as
    // they went, or Review would draw them itself instead of their own
    // appearance.
    let mut review = opencabinet_review::Review::open(sample::foreign_markups()).unwrap();
    let mut shared = Shared::start("Andrés");
    let markups = review.markups().to_vec();
    assert!(markups.iter().any(|m| m.origin.is_some()));
    shared.publish(&markups, &review.scales().clone());
    for (sent, back) in markups.iter().zip(shared.markups()) {
        assert_eq!(sent, &back);
    }
    review.take_shared(shared.markups(), shared.scales());
    assert!(review.markups().iter().all(|m| !review.draws(m)));
}

/// Calls `follow` on everyone until `done` holds.
fn follow_until(people: &mut [&mut Participant], done: impl Fn(&[&mut Participant]) -> bool) {
    let start = Instant::now();
    while !done(people) {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the session did not move: {}",
            {
                let states: Vec<String> = people
                    .iter()
                    .map(|p| {
                        let shared = p.link().shared.lock().unwrap();
                        format!(
                            "hosting {} connections {} stopped {} handed to me {} moved {:?}",
                            p.hosting(),
                            p.link().connections(),
                            p.link().stopped(),
                            shared.handed_to_me(),
                            shared
                                .moved_to()
                                .map(|m| m.chars().take(30).collect::<String>())
                        )
                    })
                    .collect();
                states.join(" | ")
            }
        );
        for p in people.iter_mut() {
            p.follow();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_host_hands_the_session_over_and_nothing_is_lost() {
    let pdf = sample::sheets(1, 2592.0, 1728.0, 2);
    let mut andres =
        Participant::host_here(Shared::start("Andrés"), pdf.clone(), "Andrés").unwrap();
    publish(andres.link(), &[cloud("a", "Andrés")]);
    let invite = andres.invite().unwrap();
    let (mut maria, got) = Participant::join_here(&invite, "María").unwrap();
    assert_eq!(got, pdf);
    let (mut juan, _) = Participant::join_here(&invite, "Juan").unwrap();
    converge(&[andres.link(), maria.link(), juan.link()], 1);
    let start = Instant::now();
    while andres.others().len() < 2 {
        assert!(start.elapsed() < Duration::from_secs(10), "nobody arrived");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(andres.others(), ["Juan", "María"]);
    assert!(maria.hand_over("Juan").is_err(), "only the host hands over");
    assert!(andres.hand_over("Nobody").is_err());

    // María takes the session; Juan and Andrés move to her computer.
    andres.hand_over("María").unwrap();
    follow_until(&mut [&mut andres, &mut maria, &mut juan], |p| {
        p[1].hosting()
            && !p[0].hosting()
            && p[2].link().connections() >= 1
            && p[0].link().connections() >= 1
    });
    let moved = maria.invite().unwrap();
    assert_ne!(moved, invite);
    assert_eq!(juan.link().shared.lock().unwrap().moved_to(), Some(moved));

    // After the old door closes, everyone still marks up together.
    std::thread::sleep(Duration::from_secs(6));
    andres.follow();
    juan.follow();
    let mut his = juan.link().shared.lock().unwrap().markups();
    his.push(cloud("j", "Juan"));
    publish(juan.link(), &his);
    converge(&[andres.link(), maria.link(), juan.link()], 2);
    assert!(!juan.link().stopped());

    // María leaves: the session goes to someone who is left.
    maria.leave();
    follow_until(&mut [&mut andres, &mut juan], |p| {
        p[0].hosting() || p[1].hosting()
    });
    andres.leave();
    juan.leave();
}

#[test]
fn a_session_on_a_server_is_joined_like_any_other() {
    use opencabinet_session::server::{Identity, Server};
    let server = Server::start(
        (Ipv4Addr::LOCALHOST, 0).into(),
        "127.0.0.1",
        &Identity::new().unwrap(),
    )
    .unwrap();
    let pdf = sample::sheets(1, 2592.0, 1728.0, 2);
    let mut shared = Shared::start("Andrés");
    shared.publish(&[cloud("a", "Andrés")], &BTreeMap::new());
    let andres =
        Participant::start_on_server(&server.address.to_string(), shared, pdf.clone(), "Andrés")
            .unwrap();
    assert!(andres.on_server());
    assert!(!andres.hosting());
    let invite = andres.invite().unwrap();
    assert!(invite.contains(":127.0.0.1:"));
    assert_eq!(server.rooms(), 1);

    let (maria, got) = Participant::join(&invite, "María").unwrap();
    assert_eq!(got, pdf);
    let mut theirs = maria.link().shared.lock().unwrap().markups();
    theirs.push(cloud("b", "María"));
    publish(maria.link(), &theirs);
    converge(&[andres.link(), maria.link()], 2);
    // The server passes changes on; it is not one of the people.
    assert_eq!(
        maria.link().shared.lock().unwrap().people(),
        ["Andrés", "María"]
    );

    // A wrong server fingerprint is refused.
    let mut wrong = server.address.clone();
    wrong.fingerprint[0] ^= 1;
    assert!(
        Participant::start_on_server(&wrong.to_string(), Shared::start("Eve"), pdf, "Eve").is_err()
    );
    andres.leave();
    maria.leave();
    server.stop();
}
