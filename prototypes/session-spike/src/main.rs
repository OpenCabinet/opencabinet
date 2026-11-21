// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! RFC-0004 §11, sessions spike. Three people share markups through
//! Automerge over TCP on this computer: one hosts, two join, both edit the
//! same markup at once without syncing, and everyone converges. The host
//! leaves, another participant takes over, and one who edited while
//! disconnected catches up. The activity log is read from the history.
//! Then a large session measures size and the time to join.
//!
//! A spike: no TLS, no invite secret, and clients pull in rounds where the
//! real host would push.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use automerge::sync::{Message, State, SyncDoc};
use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, AutoCommit, ObjType, ReadDoc, ROOT, ScalarValue, Value};

type Doc = Arc<Mutex<AutoCommit>>;

fn main() {
    // Andrés starts a session with one cloud on the sheet.
    let andres = person("Andrés");
    {
        let mut doc = andres.lock().unwrap();
        let markups = doc.put_object(ROOT, "markups", ObjType::Map).unwrap();
        add(&mut doc, &markups, "cloud-1", "cloud", "Confirm 36\" counter height");
        commit(&mut doc, "added cloud-1");
    }
    let host = Host::start(andres.clone());
    println!("Andrés hosts on {}", host.address);

    // María and Juan join: an empty document of their own, synced.
    let maria = person("María");
    let juan = person("Juan");
    let mut maria_link = Link::connect(&host.address, maria.clone());
    let mut juan_link = Link::connect(&host.address, juan.clone());
    maria_link.sync();
    juan_link.sync();
    assert_eq!(count(&maria), 1);
    assert_eq!(count(&juan), 1);

    // Both edit before either syncs, the same cloud included.
    {
        let mut doc = maria.lock().unwrap();
        let markups = markups(&doc);
        add(&mut doc, &markups, "callout-2", "callout", "Filler 3\" here");
        commit(&mut doc, "added callout-2");
        let cloud = markup(&doc, "cloud-1");
        doc.put(&cloud, "color", "green").unwrap();
        doc.put(&cloud, "x", 1200.0).unwrap();
        commit(&mut doc, "restyled and moved cloud-1");
    }
    {
        let mut doc = juan.lock().unwrap();
        let markups = markups(&doc);
        for n in 3..6 {
            add(&mut doc, &markups, &format!("length-{n}"), "length", "12'-0\"");
        }
        commit(&mut doc, "measured 3 lengths");
        let cloud = markup(&doc, "cloud-1");
        doc.put(&cloud, "color", "blue").unwrap();
        doc.put(&cloud, "text", "Confirm 36\" counter height with GC")
            .unwrap();
        commit(&mut doc, "restyled and edited cloud-1");
    }
    // Two rounds: up to the host, then the other's changes back down.
    for _ in 0..2 {
        maria_link.sync();
        juan_link.sync();
    }
    let docs = [&andres, &maria, &juan];
    assert!(same(&docs), "everyone converges");
    assert_eq!(count(&andres), 5);
    let doc = maria.lock().unwrap();
    let cloud = markup(&doc, "cloud-1");
    let colors = doc.get_all(&cloud, "color").unwrap();
    println!(
        "converged: 5 markups everywhere; cloud-1 is {} (both colors kept as a \
         conflict the window can show: {}), moved to x = {}, text \"{}\"",
        text(&doc, &cloud, "color"),
        colors.len(),
        text(&doc, &cloud, "x"),
        text(&doc, &cloud, "text"),
    );
    drop(doc);

    println!("\nactivity:");
    for line in activity(&andres) {
        println!("  {line}");
    }

    // Andrés leaves. María hosts; Juan edits while disconnected, then joins
    // her.
    host.stop();
    assert!(juan_link.try_sync().is_err(), "the old host is gone");
    let host = Host::start(maria.clone());
    {
        let mut doc = juan.lock().unwrap();
        let markups = markups(&doc);
        doc.delete(&markups, "length-5").unwrap();
        commit(&mut doc, "deleted length-5");
    }
    let mut juan_link = Link::connect(&host.address, juan.clone());
    juan_link.sync();
    assert!(same(&[&maria, &juan]));
    println!(
        "\nhandover: María hosts on {}, Juan's offline delete arrived: {} markups, \
         Andrés's cloud still there: {}",
        host.address,
        count(&maria),
        markup_exists(&maria, "cloud-1")
    );
    host.stop();

    scale();
}

/// A large session: size on disk, load time and the time for a new
/// participant to join over TCP.
fn scale() {
    let owner = person("Andrés");
    let started = Instant::now();
    {
        let mut doc = owner.lock().unwrap();
        let markups = doc.put_object(ROOT, "markups", ObjType::Map).unwrap();
        for n in 0..5000 {
            add(&mut doc, &markups, &format!("m-{n}"), "cloud", "Check this");
            if n % 10 == 9 {
                commit(&mut doc, "added 10 markups");
            }
        }
    }
    let built = started.elapsed();
    let bytes = owner.lock().unwrap().save();
    let started = Instant::now();
    let loaded = AutoCommit::load(&bytes).unwrap();
    let load = started.elapsed();
    let host = Host::start(owner.clone());
    let newcomer = person("Newcomer");
    let started = Instant::now();
    Link::connect(&host.address, newcomer.clone()).sync();
    let join = started.elapsed();
    assert_eq!(count(&newcomer), 5000);
    println!(
        "\n5000 markups in 500 changes: built in {:.0} ms, saved {:.0} kB, \
         loaded in {:.0} ms ({} keys), joined over TCP in {:.0} ms",
        built.as_secs_f64() * 1000.0,
        bytes.len() as f64 / 1000.0,
        load.as_secs_f64() * 1000.0,
        loaded.length(&markups(&loaded)),
        join.as_secs_f64() * 1000.0,
    );
    host.stop();
}

/// A person's copy of the session; their name is their actor, for the log.
fn person(name: &str) -> Doc {
    let mut doc = AutoCommit::new();
    doc.set_actor(ActorId::from(name.as_bytes()));
    Arc::new(Mutex::new(doc))
}

fn add(doc: &mut AutoCommit, markups: &automerge::ObjId, id: &str, kind: &str, text: &str) {
    let markup = doc.put_object(markups, id, ObjType::Map).unwrap();
    doc.put(&markup, "kind", kind).unwrap();
    doc.put(&markup, "page", 0_i64).unwrap();
    doc.put(&markup, "x", 1146.0).unwrap();
    doc.put(&markup, "y", 764.0).unwrap();
    doc.put(&markup, "width", 300.0).unwrap();
    doc.put(&markup, "height", 200.0).unwrap();
    doc.put(&markup, "color", "red").unwrap();
    doc.put(&markup, "text", text).unwrap();
}

fn commit(doc: &mut AutoCommit, message: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    doc.commit_with(
        CommitOptions::default()
            .with_message(message)
            .with_time(now.as_secs().try_into().unwrap()),
    );
}

fn markups(doc: &AutoCommit) -> automerge::ObjId {
    match doc.get(ROOT, "markups").unwrap() {
        Some((Value::Object(ObjType::Map), id)) => id,
        _ => panic!("no markups"),
    }
}

fn markup(doc: &AutoCommit, id: &str) -> automerge::ObjId {
    match doc.get(markups(doc), id).unwrap() {
        Some((Value::Object(ObjType::Map), id)) => id,
        _ => panic!("no markup {id}"),
    }
}

fn markup_exists(doc: &Doc, id: &str) -> bool {
    let doc = doc.lock().unwrap();
    doc.get(markups(&doc), id).unwrap().is_some()
}

fn text(doc: &AutoCommit, obj: &automerge::ObjId, key: &str) -> String {
    match doc.get(obj, key).unwrap() {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Str(s) => s.to_string(),
            ScalarValue::F64(f) => format!("{f}"),
            other => other.to_string(),
        },
        other => format!("{other:?}"),
    }
}

fn count(doc: &Doc) -> usize {
    let doc = doc.lock().unwrap();
    doc.length(markups(&doc))
}

fn same(docs: &[&Doc]) -> bool {
    let heads: Vec<_> = docs
        .iter()
        .map(|d| {
            let mut heads = d.lock().unwrap().get_heads();
            heads.sort();
            heads
        })
        .collect();
    heads.windows(2).all(|pair| pair[0] == pair[1])
}

/// Who changed what, and when, in the order Automerge applied it.
fn activity(doc: &Doc) -> Vec<String> {
    let mut doc = doc.lock().unwrap();
    doc.get_changes(&[])
        .iter()
        .map(|change| {
            let who = String::from_utf8_lossy(change.actor_id().to_bytes()).into_owned();
            let ago = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - change.timestamp();
            format!(
                "{who:<7} {:<30} {ago} s ago",
                change.message().unwrap_or("(no message)")
            )
        })
        .collect()
}

/// A host: accepts participants and syncs its copy with each.
struct Host {
    address: String,
    streams: Arc<Mutex<Vec<TcpStream>>>,
}

impl Host {
    fn start(doc: Doc) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let streams = Arc::new(Mutex::new(Vec::new()));
        let open = streams.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                open.lock().unwrap().push(stream.try_clone().unwrap());
                let doc = doc.clone();
                std::thread::spawn(move || {
                    let mut state = State::new();
                    // Each frame from the participant gets one back.
                    while let Ok(frame) = read(&mut stream) {
                        let mut doc = doc.lock().unwrap();
                        if !frame.is_empty() {
                            let message = Message::decode(&frame).unwrap();
                            doc.sync().receive_sync_message(&mut state, message).unwrap();
                        }
                        let reply = doc.sync().generate_sync_message(&mut state);
                        drop(doc);
                        if write(&mut stream, reply.map(Message::encode)).is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self { address, streams }
    }

    /// Leaves: every participant's connection closes.
    fn stop(self) {
        for stream in self.streams.lock().unwrap().iter() {
            stream.shutdown(Shutdown::Both).ok();
        }
    }
}

/// A participant's connection to a host.
struct Link {
    stream: TcpStream,
    doc: Doc,
    state: State,
}

impl Link {
    fn connect(address: &str, doc: Doc) -> Self {
        Self {
            stream: TcpStream::connect(address).unwrap(),
            doc,
            state: State::new(),
        }
    }

    fn sync(&mut self) {
        self.try_sync().unwrap();
    }

    /// Exchanges messages until neither side has anything to send.
    fn try_sync(&mut self) -> std::io::Result<()> {
        loop {
            let message = self.doc.lock().unwrap().sync().generate_sync_message(&mut self.state);
            let sent = message.is_some();
            write(&mut self.stream, message.map(Message::encode))?;
            let reply = read(&mut self.stream)?;
            if reply.is_empty() {
                if !sent {
                    return Ok(());
                }
                continue;
            }
            let reply = Message::decode(&reply).unwrap();
            self.doc
                .lock()
                .unwrap()
                .sync()
                .receive_sync_message(&mut self.state, reply)
                .unwrap();
        }
    }
}

/// A frame: its length, then its bytes; empty for nothing to say.
fn write(stream: &mut TcpStream, frame: Option<Vec<u8>>) -> std::io::Result<()> {
    let frame = frame.unwrap_or_default();
    stream.write_all(&u32::try_from(frame.len()).unwrap().to_be_bytes())?;
    stream.write_all(&frame)
}

fn read(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let mut frame = vec![0; u32::from_be_bytes(length) as usize];
    stream.read_exact(&mut frame)?;
    Ok(frame)
}
