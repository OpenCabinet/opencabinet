// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The session document: everyone's markups, the pages' scales and who
//! is in the session, in one Automerge document that every participant
//! keeps a whole copy of (RFC-0004 §6).
//!
//! Each markup is stored whole, as JSON under its id, so two people
//! changing the same markup at once end with one of the two versions on
//! every copy. Every change says who made it, when, and what it did, which
//! is the activity log. The chat is a list in the same document.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use automerge::sync::{Message, State, SyncDoc};
use automerge::transaction::{CommitOptions, Transactable};
use automerge::{ActorId, AutoCommit, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value};
use opencabinet_model::Length;
use opencabinet_pdf::Point;
use opencabinet_review::{Kind, Markup, Scale, Status};
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
        for key in [MARKUPS, SCALES, PEOPLE] {
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

    /// The markups, in the order of their ids.
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
    })
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
