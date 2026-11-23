// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The session document: everyone's markups, the pages' scales and who
//! is in the session, in one Automerge document that every participant
//! keeps a whole copy of (RFC-0004 §6).
//!
//! Each markup is stored whole, as JSON under its id, so two people
//! changing the same markup at once end with one of the two versions on
//! every copy. Replies are stored one by one, apart from their markup, so
//! two people answering at once both keep their reply. Every change says
//! who made it, when, and what it did, which is the activity log. The chat
//! is a list in the same document.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use automerge::sync::{Message, State, SyncDoc};
use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, AutoCommit, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value};
use opencabinet_model::Length;
use opencabinet_pdf::Point;
use opencabinet_review::{Kind, Markup, Reply, Scale, Status};
use serde_json::{Value as Json, json};

/// Something someone did in the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    /// Who.
    pub who: String,
    /// When, in seconds since 1970.
    pub when: i64,
    /// What, such as `added a cloud on page 2`.
    pub what: String,
}

/// A chat message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    /// Who wrote it.
    pub who: String,
    /// When, in seconds since 1970.
    pub when: i64,
    /// What they wrote.
    pub text: String,
}

/// One participant's copy of the session.
#[derive(Debug)]
pub struct Shared {
    doc: AutoCommit,
}

const MARKUPS: &str = "markups";
const SCALES: &str = "scales";
const PEOPLE: &str = "people";
const CHAT: &str = "chat";
const REPLIES: &str = "replies";
/// Who the host handed the session over to, and where it went.
const HANDOVER: &str = "handover";
/// The longest chat message kept.
const MAX_SAID: usize = 4000;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

impl Shared {
    /// A new session, started by `name`.
    #[must_use]
    pub fn start(name: &str) -> Self {
        let mut doc = AutoCommit::new();
        doc.set_actor(ActorId::random());
        for key in [MARKUPS, SCALES, PEOPLE, REPLIES] {
            // A fresh document always takes a new map.
            let _ = doc.put_object(ROOT, key, ObjType::Map);
        }
        let _ = doc.put_object(ROOT, CHAT, ObjType::List);
        let mut shared = Self { doc };
        shared.introduce(name, "started the session");
        shared
    }

    /// A copy of the session from what the host sent, for `name` joining
    /// it.
    ///
    /// # Errors
    /// When the bytes are not a session.
    pub fn join(bytes: &[u8], name: &str) -> Result<Self, String> {
        let mut doc = AutoCommit::load(bytes).map_err(|e| e.to_string())?;
        doc.set_actor(ActorId::random());
        let mut shared = Self { doc };
        if shared.map(MARKUPS).is_none() {
            return Err("this is not a review session".into());
        }
        shared.introduce(name, "joined the session");
        Ok(shared)
    }

    /// A copy of the session for a server that only passes it on: it does
    /// not join it as a person.
    ///
    /// # Errors
    /// When the bytes are not a session.
    pub fn relay(bytes: &[u8]) -> Result<Self, String> {
        let mut doc = AutoCommit::load(bytes).map_err(|e| e.to_string())?;
        doc.set_actor(ActorId::random());
        let shared = Self { doc };
        if shared.map(MARKUPS).is_none() {
            return Err("this is not a review session".into());
        }
        Ok(shared)
    }

    /// Hands the session over to the person called `name`, who will host
    /// it from now on; false when nobody else has that name.
    pub fn hand_over(&mut self, name: &str) -> bool {
        let me = self.actor();
        let Some(people) = self.map(PEOPLE) else {
            return false;
        };
        let Some(actor) = self
            .strings(&people)
            .into_iter()
            .find(|(actor, n)| n == name && *actor != me)
            .map(|(actor, _)| actor)
        else {
            return false;
        };
        let Ok(handover) = self.doc.put_object(ROOT, HANDOVER, ObjType::Map) else {
            return false;
        };
        let _ = self.doc.put(&handover, "to", actor);
        let _ = self.doc.put(&handover, "invite", "");
        self.doc.commit_with(
            CommitOptions::default()
                .with_message(format!("handed the session over to {name}"))
                .with_time(now()),
        );
        true
    }

    /// Whether the session was handed over to this participant, who has
    /// not started hosting it yet.
    #[must_use]
    pub fn handed_to_me(&self) -> bool {
        self.map(HANDOVER).is_some_and(|map| {
            let found = self.strings(&map);
            found.get("to") == Some(&self.actor())
                && found.get("invite").is_none_or(String::is_empty)
        })
    }

    /// Says the session now lives at `invite`, hosted here.
    pub fn moved_here(&mut self, invite: &str) {
        let Some(handover) = self.map(HANDOVER) else {
            return;
        };
        let _ = self.doc.put(&handover, "invite", invite);
        self.doc.commit_with(
            CommitOptions::default()
                .with_message("hosts the session now")
                .with_time(now()),
        );
    }

    /// Where the session moved when it was last handed over, if it was.
    #[must_use]
    pub fn moved_to(&self) -> Option<String> {
        let map = self.map(HANDOVER)?;
        self.strings(&map)
            .remove("invite")
            .filter(|invite| !invite.is_empty())
    }

    /// The others in the session, by name.
    #[must_use]
    pub fn others(&self) -> Vec<String> {
        let Some(map) = self.map(PEOPLE) else {
            return Vec::new();
        };
        let me = self.actor();
        let mine = self.own_name();
        let mut names: Vec<String> = self
            .strings(&map)
            .into_iter()
            .filter(|(actor, name)| *actor != me && *name != mine)
            .map(|(_, name)| name)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The whole session, to send to someone joining.
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    fn actor(&self) -> String {
        self.doc.get_actor().to_hex_string()
    }

    fn introduce(&mut self, name: &str, what: &str) {
        let Some(people) = self.map(PEOPLE) else {
            return;
        };
        let actor = self.actor();
        let _ = self.doc.put(&people, actor, name);
        self.doc
            .commit_with(CommitOptions::default().with_message(what).with_time(now()));
    }

    /// This participant's name in the session.
    fn own_name(&self) -> String {
        self.map(PEOPLE)
            .and_then(|map| self.strings(&map).remove(&self.actor()))
            .unwrap_or_else(|| "Someone".into())
    }

    fn map(&self, key: &str) -> Option<ObjId> {
        self.object(key, ObjType::Map)
    }

    fn object(&self, key: &str, kind: ObjType) -> Option<ObjId> {
        match self.doc.get(ROOT, key).ok()?? {
            (Value::Object(found), id) if found == kind => Some(id),
            _ => None,
        }
    }

    /// Writes `text` in the chat; whether it was written.
    pub fn say(&mut self, text: &str) -> bool {
        let text = text.trim();
        let Some(chat) = self.object(CHAT, ObjType::List) else {
            return false;
        };
        if text.is_empty() {
            return false;
        }
        let text: String = text.chars().take(MAX_SAID).collect();
        let said = json!({ "who": self.own_name(), "when": now(), "text": text });
        let end = self.doc.length(&chat);
        if self.doc.insert(&chat, end, said.to_string()).is_err() {
            return false;
        }
        // No message: the chat is not part of the activity log.
        self.doc
            .commit_with(CommitOptions::default().with_time(now()));
        true
    }

    /// The chat, oldest first.
    #[must_use]
    pub fn chat(&self) -> Vec<Said> {
        let Some(chat) = self.object(CHAT, ObjType::List) else {
            return Vec::new();
        };
        (0..self.doc.length(&chat))
            .filter_map(|i| match self.doc.get(&chat, i).ok()?? {
                (Value::Scalar(value), _) => match value.as_ref() {
                    ScalarValue::Str(text) => {
                        let json: Json = serde_json::from_str(text).ok()?;
                        Some(Said {
                            who: json.get("who")?.as_str()?.to_owned(),
                            when: json.get("when")?.as_i64()?,
                            text: json.get("text")?.as_str()?.to_owned(),
                        })
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn strings(&self, map: &ObjId) -> BTreeMap<String, String> {
        self.doc
            .keys(map)
            .filter_map(|key| match self.doc.get(map, &key).ok()?? {
                (Value::Scalar(value), _) => match value.as_ref() {
                    ScalarValue::Str(text) => Some((key, text.to_string())),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    /// Writes what differs between `markups` and `scales` and the
    /// session, as one change; whether anything did.
    pub fn publish(&mut self, markups: &[Markup], scales: &BTreeMap<usize, Scale>) -> bool {
        let (Some(map), Some(scale_map)) = (self.map(MARKUPS), self.map(SCALES)) else {
            return false;
        };
        let stored = self.strings(&map);
        let mut said = Vec::new();
        for markup in markups {
            let text = encode(markup).to_string();
            match stored.get(&markup.id) {
                Some(old) if *old == text => {}
                Some(_) => {
                    let _ = self.doc.put(&map, &markup.id, text);
                    said.push(format!("changed {}", describe(markup)));
                }
                None => {
                    let _ = self.doc.put(&map, &markup.id, text);
                    said.push(format!("added {}", describe(markup)));
                }
            }
        }
        for (id, old) in &stored {
            if !markups.iter().any(|m| &m.id == id) {
                let _ = self.doc.delete(&map, id.as_str());
                let what = decode(id, old).map_or_else(|| "a markup".to_owned(), |m| describe(&m));
                said.push(format!("deleted {what}"));
            }
        }
        said.extend(self.publish_replies(markups));
        let stored = self.strings(&scale_map);
        for (page, scale) in scales {
            let text = format!("{} {}", scale.points, scale.real.nm());
            if stored.get(&page.to_string()) != Some(&text) {
                let _ = self.doc.put(&scale_map, page.to_string(), text);
                said.push(format!("set the scale of page {}", page + 1));
            }
        }
        if said.is_empty() {
            return false;
        }
        let message = if said.len() > 3 {
            format!("{}, and {} more", said[..2].join(", "), said.len() - 2)
        } else {
            said.join(", ")
        };
        self.doc.commit_with(
            CommitOptions::default()
                .with_message(message)
                .with_time(now()),
        );
        true
    }

    /// Writes the replies that differ from the session's; what was said
    /// about them for the log.
    fn publish_replies(&mut self, markups: &[Markup]) -> Vec<String> {
        let Some(map) = self.map(REPLIES) else {
            return Vec::new();
        };
        let stored = self.strings(&map);
        let mut said = Vec::new();
        let mut current = std::collections::BTreeSet::new();
        for markup in markups {
            let mut answered = false;
            for (order, reply) in markup.replies.iter().enumerate() {
                let key = format!("{}/{}", markup.id, reply_key(reply, order));
                let text = encode_reply(reply, order).to_string();
                if stored.get(&key) != Some(&text) {
                    let _ = self.doc.put(&map, key.as_str(), text);
                    answered |= !stored.contains_key(&key);
                }
                current.insert(key);
            }
            if answered {
                said.push(format!("replied to {}", describe(markup)));
            }
        }
        for key in stored.keys() {
            if !current.contains(key) {
                let _ = self.doc.delete(&map, key.as_str());
                if let Some(markup) = key
                    .split_once('/')
                    .and_then(|(id, _)| markups.iter().find(|m| m.id == id))
                {
                    said.push(format!("took back a reply to {}", describe(markup)));
                }
            }
        }
        said
    }

    /// The markups, in the order of their ids, each with its replies.
    #[must_use]
    pub fn markups(&self) -> Vec<Markup> {
        let Some(map) = self.map(MARKUPS) else {
            return Vec::new();
        };
        let mut markups: Vec<Markup> = self
            .strings(&map)
            .iter()
            .filter_map(|(id, text)| decode(id, text))
            .collect();
        // Made-earlier first: ids are time ordered (UUIDv7).
        markups.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(map) = self.map(REPLIES) {
            let mut replies: BTreeMap<&str, Vec<(u64, String, Reply)>> = BTreeMap::new();
            let stored = self.strings(&map);
            for (key, text) in &stored {
                let Some((id, rest)) = key.split_once('/') else {
                    continue;
                };
                if let Some((order, reply)) = decode_reply(text) {
                    replies
                        .entry(id)
                        .or_default()
                        .push((order, rest.to_owned(), reply));
                }
            }
            for markup in &mut markups {
                if let Some(mut found) = replies.remove(markup.id.as_str()) {
                    // In the order they were given; two at once by key.
                    found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
                    markup.replies = found.into_iter().map(|(_, _, r)| r).collect();
                }
            }
        }
        markups
    }

    /// The pages' scales.
    #[must_use]
    pub fn scales(&self) -> BTreeMap<usize, Scale> {
        let Some(map) = self.map(SCALES) else {
            return BTreeMap::new();
        };
        self.strings(&map)
            .iter()
            .filter_map(|(page, text)| {
                let (points, nm) = text.split_once(' ')?;
                let points: f64 = points.parse().ok()?;
                let real = Length::from_nm(nm.parse().ok()?);
                Some((page.parse().ok()?, Scale { points, real }))
            })
            .collect()
    }

    /// Who is in the session, by name.
    #[must_use]
    pub fn people(&self) -> Vec<String> {
        let Some(map) = self.map(PEOPLE) else {
            return Vec::new();
        };
        let mut names: Vec<String> = self.strings(&map).into_values().collect();
        names.sort();
        names.dedup();
        names
    }

    /// What everyone did, oldest first.
    pub fn activity(&mut self) -> Vec<Activity> {
        let names = self
            .map(PEOPLE)
            .map(|map| self.strings(&map))
            .unwrap_or_default();
        self.doc
            .get_changes(&[])
            .iter()
            .filter_map(|change| {
                let what = change.message()?.to_owned();
                let who = names
                    .get(&change.actor_id().to_hex_string())
                    .cloned()
                    .unwrap_or_else(|| "Someone".into());
                Some(Activity {
                    who,
                    when: change.timestamp(),
                    what,
                })
            })
            .collect()
    }

    /// The next message to send to a peer whose state is `state`, if any.
    pub fn outgoing(&mut self, state: &mut State) -> Option<Vec<u8>> {
        self.doc
            .sync()
            .generate_sync_message(state)
            .map(Message::encode)
    }

    /// Takes in a message from a peer; whether the session changed.
    ///
    /// # Errors
    /// When the message cannot be read.
    pub fn incoming(&mut self, state: &mut State, bytes: &[u8]) -> Result<bool, String> {
        let before = self.doc.get_heads();
        let message = Message::decode(bytes).map_err(|e| e.to_string())?;
        self.doc
            .sync()
            .receive_sync_message(state, message)
            .map_err(|e| e.to_string())?;
        Ok(self.doc.get_heads() != before)
    }
}

/// `a cloud on page 2`.
fn describe(markup: &Markup) -> String {
    let kind = markup.kind.label().to_lowercase();
    let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {kind} on page {}", markup.page + 1)
}

fn encode(markup: &Markup) -> Json {
    json!({
        "page": markup.page,
        "kind": markup.kind.key(),
        "points": markup.points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
        "color": markup.color,
        "width": markup.width,
        "fill": markup.fill,
        "text": markup.text,
        "subject": markup.subject,
        "status": markup.status.label(),
        "author": markup.author,
        "modified": markup.modified,
        "scale": markup.scale.map(|s| (s.points, s.real.nm())),
        "origin": markup.origin,
        "look": markup.look,
    })
}

/// What tells a reply apart among its markup's: its name, or the object
/// it was read from, or its place.
fn reply_key(reply: &Reply, order: usize) -> String {
    if !reply.name.is_empty() && !reply.name.contains('/') {
        reply.name.clone()
    } else if let Some((number, generation)) = reply.origin {
        format!("object-{number}-{generation}")
    } else {
        format!("at-{order}")
    }
}

fn encode_reply(reply: &Reply, order: usize) -> Json {
    json!({
        "order": order,
        "name": reply.name,
        "author": reply.author,
        "contents": reply.contents,
        "modified": reply.modified,
        "origin": reply.origin,
    })
}

fn decode_reply(text: &str) -> Option<(u64, Reply)> {
    let json: Json = serde_json::from_str(text).ok()?;
    let string = |key: &str| {
        json.get(key)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let origin = json.get("origin").and_then(|o| {
        let number = u32::try_from(o.get(0)?.as_u64()?).ok()?;
        let generation = u16::try_from(o.get(1)?.as_u64()?).ok()?;
        Some((number, generation))
    });
    Some((
        json.get("order").and_then(Json::as_u64).unwrap_or(0),
        Reply {
            name: string("name"),
            author: string("author"),
            contents: string("contents"),
            modified: string("modified"),
            origin,
        },
    ))
}

fn decode(id: &str, text: &str) -> Option<Markup> {
    let json: Json = serde_json::from_str(text).ok()?;
    let number = |key: &str| json.get(key)?.as_f64();
    let string = |key: &str| {
        json.get(key)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let points = json
        .get("points")?
        .as_array()?
        .iter()
        .filter_map(|p| Some(Point::new(p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
        .collect();
    let color = json.get("color")?.as_array()?;
    let channel = |i: usize| color.get(i).and_then(Json::as_f64).unwrap_or(0.0);
    let scale = json.get("scale").and_then(|s| {
        let points = s.get(0)?.as_f64()?;
        let nm = s.get(1)?.as_i64()?;
        Some(Scale {
            points,
            real: Length::from_nm(nm),
        })
    });
    let origin = json.get("origin").and_then(|o| {
        let number = u32::try_from(o.get(0)?.as_u64()?).ok()?;
        let generation = u16::try_from(o.get(1)?.as_u64()?).ok()?;
        Some((number, generation))
    });
    Some(Markup {
        id: id.to_owned(),
        page: usize::try_from(json.get("page")?.as_u64()?).ok()?,
        kind: Kind::from_key(json.get("kind")?.as_str()?)?,
        points,
        color: [channel(0), channel(1), channel(2)],
        width: number("width").unwrap_or(2.0),
        fill: json.get("fill").and_then(Json::as_bool).unwrap_or(false),
        text: string("text"),
        subject: string("subject"),
        status: Status::from_label(&string("status")),
        author: string("author"),
        modified: string("modified"),
        scale,
        origin,
        replies: Vec::new(),
        look: string("look"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(id: &str, page: usize) -> Markup {
        Markup {
            id: id.into(),
            page,
            kind: Kind::Cloud,
            points: vec![
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 5.0),
            ],
            text: "Check".into(),
            author: "Andrés".into(),
            origin: Some((12, 0)),
            ..Markup::default()
        }
    }

    #[test]
    fn a_markup_comes_back_as_it_went_in() {
        let mut markup = cloud("a", 1);
        markup.status = Status::Accepted;
        markup.scale = Some(Scale {
            points: 18.0,
            real: Length::from_inches(12),
        });
        let back = decode("a", &encode(&markup).to_string()).unwrap();
        assert_eq!(back, markup);
    }

    #[test]
    fn changes_are_written_once_and_logged_by_name() {
        let mut shared = Shared::start("Andrés");
        let mut markups = vec![cloud("a", 0)];
        assert!(shared.publish(&markups, &BTreeMap::new()));
        assert!(!shared.publish(&markups, &BTreeMap::new()));
        markups[0].text = "Checked".into();
        markups.push(cloud("b", 1));
        assert!(shared.publish(&markups, &BTreeMap::new()));
        markups.remove(0);
        assert!(shared.publish(&markups, &BTreeMap::new()));
        assert_eq!(shared.markups(), markups);
        let what: Vec<String> = shared.activity().into_iter().map(|a| a.what).collect();
        assert_eq!(
            what,
            [
                "started the session",
                "added a cloud on page 1",
                "changed a cloud on page 1, added a cloud on page 2",
                "deleted a cloud on page 1",
            ]
        );
        assert!(shared.activity().iter().all(|a| a.who == "Andrés"));
    }

    #[test]
    fn replies_given_at_once_are_both_kept() {
        let mut host = Shared::start("Andrés");
        let markups = vec![cloud("01", 0)];
        assert!(host.publish(&markups, &BTreeMap::new()));
        let mut guest = Shared::join(&host.save(), "Sam").unwrap();
        let answer = |name: &str, who: &str| {
            let mut answered = markups.clone();
            answered[0].replies.push(Reply {
                name: name.into(),
                author: who.into(),
                contents: format!("{who} agrees"),
                ..Reply::default()
            });
            answered
        };
        assert!(host.publish(&answer("r-a", "Andrés"), &BTreeMap::new()));
        assert!(guest.publish(&answer("r-b", "Sam"), &BTreeMap::new()));
        let (mut a, mut b) = (State::new(), State::new());
        for _ in 0..8 {
            if let Some(m) = host.outgoing(&mut a) {
                guest.incoming(&mut b, &m).unwrap();
            }
            if let Some(m) = guest.outgoing(&mut b) {
                host.incoming(&mut a, &m).unwrap();
            }
        }
        let on_host = host.markups();
        assert_eq!(on_host, guest.markups());
        let who: Vec<&str> = on_host[0]
            .replies
            .iter()
            .map(|r| r.author.as_str())
            .collect();
        assert_eq!(who, ["Andrés", "Sam"]);
        assert!(
            host.activity()
                .iter()
                .any(|a| a.who == "Sam" && a.what == "replied to a cloud on page 1")
        );
        // Taking one back reaches the log too.
        let mut fewer = on_host.clone();
        fewer[0].replies.remove(1);
        assert!(guest.publish(&fewer, &BTreeMap::new()));
        assert_eq!(guest.markups(), fewer);
        assert!(
            guest
                .activity()
                .iter()
                .any(|a| a.what == "took back a reply to a cloud on page 1")
        );
    }

    #[test]
    fn the_chat_reaches_the_other_copy_and_stays_out_of_the_log() {
        let mut host = Shared::start("Andrés");
        let mut guest = Shared::join(&host.save(), "María").unwrap();
        assert!(guest.say("  Is the sink centered?  "));
        assert!(!guest.say("   "));
        let (mut a, mut b) = (State::new(), State::new());
        for _ in 0..6 {
            if let Some(m) = guest.outgoing(&mut a) {
                host.incoming(&mut b, &m).unwrap();
            }
            if let Some(m) = host.outgoing(&mut b) {
                guest.incoming(&mut a, &m).unwrap();
            }
        }
        assert!(host.say("Yes, on the window."));
        let chat = host.chat();
        assert_eq!(chat.len(), 2);
        assert_eq!(
            (chat[0].who.as_str(), chat[0].text.as_str()),
            ("María", "Is the sink centered?")
        );
        assert_eq!(chat[1].who, "Andrés");
        let what: Vec<String> = host.activity().into_iter().map(|a| a.what).collect();
        assert_eq!(what, ["started the session", "joined the session"]);
    }

    #[test]
    fn two_copies_meet_through_sync_messages() {
        let mut host = Shared::start("Andrés");
        host.publish(&[cloud("a", 0)], &BTreeMap::new());
        let mut guest = Shared::join(&host.save(), "María").unwrap();
        guest.publish(
            &[cloud("a", 0), cloud("b", 0)],
            &BTreeMap::from([(
                0,
                Scale {
                    points: 18.0,
                    real: Length::from_inches(12),
                },
            )]),
        );
        let (mut to_guest, mut to_host) = (State::new(), State::new());
        for _ in 0..6 {
            if let Some(m) = host.outgoing(&mut to_guest) {
                guest.incoming(&mut to_host, &m).unwrap();
            }
            if let Some(m) = guest.outgoing(&mut to_host) {
                host.incoming(&mut to_guest, &m).unwrap();
            }
        }
        assert_eq!(host.markups().len(), 2);
        assert_eq!(host.scales().len(), 1);
        assert_eq!(host.people(), ["Andrés", "María"]);
        assert!(
            host.activity()
                .iter()
                .any(|a| a.who == "María" && a.what.contains("added a cloud"))
        );
    }
}
