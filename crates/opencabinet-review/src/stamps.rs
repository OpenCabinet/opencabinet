// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stamps Review comes with: the words a reviewer answers a drawing
//! with, each in a color of its own. A stamp's words can be changed after
//! it is placed, and a shop can add stamps of its own.
//!
//! A stamp's words may hold fields filled in when it is placed (see
//! [`Fields::fill`]): `{name}`, `{date}` or `{date:dd/MM/yyyy}`, `{time}`,
//! `{page}`, `{sheet}`, `{file}`, and `{ask:Job number}`, whose value the
//! person is asked for.
//!
//! A [`Design`] is a stamp of one's own: lines of words, each its size and
//! weight, how they are aligned, its border and whether it is filled. It
//! is kept as a small JSON file, `.ocstamp`, and travels in the PDF with
//! the stamp, in Review's own entries, as its [`Design::look`].

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::Color;

/// A stamp to place: its words and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Template {
    /// What it says.
    pub words: &'static str,
    /// Its color.
    pub color: Color,
}

const GREEN: Color = [0.1, 0.5, 0.2];
const BLUE: Color = [0.1, 0.3, 0.75];
const ORANGE: Color = [0.85, 0.45, 0.0];
const RED: Color = [0.8, 0.1, 0.1];
const GRAY: Color = [0.35, 0.35, 0.35];

/// The stamps, in the order the window offers them.
pub const TEMPLATES: [Template; 9] = [
    Template {
        words: "APPROVED",
        color: GREEN,
    },
    Template {
        words: "APPROVED AS NOTED",
        color: BLUE,
    },
    Template {
        words: "REVISE AND RESUBMIT",
        color: ORANGE,
    },
    Template {
        words: "REJECTED",
        color: RED,
    },
    Template {
        words: "REVIEWED",
        color: BLUE,
    },
    Template {
        words: "FOR INFORMATION ONLY",
        color: GRAY,
    },
    Template {
        words: "NOT FOR CONSTRUCTION",
        color: RED,
    },
    Template {
        words: "FIELD VERIFY",
        color: ORANGE,
    },
    Template {
        words: "REVIEWED BY {name} {date}",
        color: BLUE,
    },
];

/// The longest a stamp's words may be.
pub const MAX_WORDS: usize = 120;

/// `words` with `{name}` and `{date}` filled in.
#[must_use]
pub fn fill_in(words: &str, name: &str, date: &str) -> String {
    words.replace("{name}", name).replace("{date}", date)
}

/// A stamp's size when placed with a click, in points.
pub const SIZE: (f64, f64) = (216.0, 64.0);

/// What a `.ocstamp` file says it is.
const FORMAT: &str = "opencabinet-review-stamp";
/// The most lines a stamp has.
pub const MAX_LINES: usize = 8;
/// Room around a stamp's words, in points.
const PADDING: f64 = 8.0;

/// How a stamp's lines are lined up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// On the left.
    Left,
    /// In the middle.
    #[default]
    Center,
    /// On the right.
    Right,
}

/// A stamp's border.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Border {
    /// None.
    None,
    /// A rectangle.
    Square,
    /// A rectangle with rounded corners.
    #[default]
    Rounded,
}

impl Align {
    /// Every way, in the order the editor offers them.
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    /// What the editor calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::Left => "l",
            Self::Center => "c",
            Self::Right => "r",
        }
    }
}

impl Border {
    /// Every border, in the order the editor offers them.
    pub const ALL: [Self; 3] = [Self::None, Self::Square, Self::Rounded];

    /// What the editor calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Square => "Square",
            Self::Rounded => "Rounded",
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::None => "n",
            Self::Square => "s",
            Self::Rounded => "r",
        }
    }
}

/// A line of a stamp.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Its words, fields included.
    pub text: String,
    /// Its size, in points, when the stamp is at its own size.
    pub size: f64,
    /// Whether it is bold.
    pub bold: bool,
}

/// A stamp of one's own.
#[derive(Clone, Debug, PartialEq)]
pub struct Design {
    /// What it is called in the list.
    pub name: String,
    /// Its lines, top to bottom.
    pub lines: Vec<Line>,
    /// How they are lined up.
    pub align: Align,
    /// Its border.
    pub border: Border,
    /// Its color: words and border.
    pub color: Color,
    /// Whether it is filled with its color, see through.
    pub fill: bool,
}

impl Default for Design {
    fn default() -> Self {
        Self {
            name: "New stamp".into(),
            lines: vec![
                Line {
                    text: "CHECKED".into(),
                    size: 20.0,
                    bold: true,
                },
                Line {
                    text: "{name} · {date}".into(),
                    size: 9.0,
                    bold: false,
                },
            ],
            align: Align::Center,
            border: Border::Rounded,
            color: BLUE,
            fill: false,
        }
    }
}

impl Design {
    /// Review's built-in stamp `template` as a design: its words large,
    /// who and when small under them.
    #[must_use]
    pub fn from_template(template: &Template) -> Self {
        let mut lines = vec![Line {
            text: template.words.to_owned(),
            size: 20.0,
            bold: true,
        }];
        if !template.words.contains('{') {
            lines.push(Line {
                text: "{name} · {date}".into(),
                size: 9.0,
                bold: false,
            });
        }
        Self {
            name: template.words.to_owned(),
            lines,
            align: Align::Center,
            border: Border::Rounded,
            color: template.color,
            fill: false,
        }
    }

    /// Its lines' words, one per line, fields not filled in.
    #[must_use]
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// All but the words, written short, to keep with a placed stamp:
    /// `a=c;b=r;s=20,9;w=1,0`.
    #[must_use]
    pub fn look(&self) -> String {
        let sizes: Vec<String> = self.lines.iter().map(|l| format!("{}", l.size)).collect();
        let bold: Vec<&str> = self
            .lines
            .iter()
            .map(|l| if l.bold { "1" } else { "0" })
            .collect();
        format!(
            "a={};b={};s={};w={}",
            self.align.key(),
            self.border.key(),
            sizes.join(","),
            bold.join(",")
        )
    }

    /// The design a placed stamp keeps: its words, one line each, and its
    /// look; `None` when the look cannot be read.
    #[must_use]
    pub fn from_look(text: &str, look: &str, color: Color, fill: bool) -> Option<Self> {
        let parts: BTreeMap<&str, &str> =
            look.split(';').filter_map(|p| p.split_once('=')).collect();
        let align = Align::ALL
            .into_iter()
            .find(|a| Some(a.key()) == parts.get("a").copied())?;
        let border = Border::ALL
            .into_iter()
            .find(|b| Some(b.key()) == parts.get("b").copied())?;
        let sizes: Vec<f64> = parts
            .get("s")?
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();
        let bold: Vec<bool> = parts.get("w")?.split(',').map(|b| b == "1").collect();
        let lines = text
            .lines()
            .enumerate()
            .take(MAX_LINES)
            .map(|(i, text)| Line {
                text: text.to_owned(),
                size: sizes.get(i).copied().unwrap_or(10.0).clamp(4.0, 96.0),
                bold: bold.get(i).copied().unwrap_or(false),
            })
            .collect();
        Some(Self {
            name: String::new(),
            lines,
            align,
            border,
            color,
            fill,
        })
    }

    /// Its own size, in points, with these words in it.
    #[must_use]
    pub fn size_with(&self, text: &str) -> (f64, f64) {
        let widths = text.lines().zip(&self.lines).map(|(words, line)| {
            crate::text_width(words, line.size) * if line.bold { 1.08 } else { 1.0 }
        });
        let width = widths.fold(0.0_f64, f64::max);
        let height: f64 = self.lines.iter().map(|l| l.size * 1.25).sum();
        (
            (width + PADDING * 2.0).max(48.0),
            (height + PADDING * 2.0).max(24.0),
        )
    }

    /// As kept in a `.ocstamp` file.
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "format": FORMAT,
            "version": 1,
            "name": self.name,
            "lines": self.lines.iter().map(|l| json!({
                "text": l.text, "size": l.size, "bold": l.bold,
            })).collect::<Vec<_>>(),
            "align": self.align.key(),
            "border": self.border.key(),
            "color": self.color,
            "fill": self.fill,
        })
    }

    /// The design a `.ocstamp` file holds; `None` when it is not one.
    #[must_use]
    pub fn from_json(json: &Json) -> Option<Self> {
        if json.get("format").and_then(Json::as_str) != Some(FORMAT)
            || json.get("version").and_then(Json::as_u64) != Some(1)
        {
            return None;
        }
        let color = json.get("color")?.as_array()?;
        let channel = |i: usize| -> Option<f64> { Some(color.get(i)?.as_f64()?.clamp(0.0, 1.0)) };
        let lines: Vec<Line> = json
            .get("lines")?
            .as_array()?
            .iter()
            .take(MAX_LINES)
            .filter_map(|l| {
                Some(Line {
                    text: l.get("text")?.as_str()?.chars().take(MAX_WORDS).collect(),
                    size: l.get("size")?.as_f64()?.clamp(4.0, 96.0),
                    bold: l.get("bold").and_then(Json::as_bool).unwrap_or(false),
                })
            })
            .collect();
        if lines.is_empty() {
            return None;
        }
        let key = |k: &str| json.get(k).and_then(Json::as_str).unwrap_or_default();
        Some(Self {
            name: json
                .get("name")?
                .as_str()?
                .trim()
                .chars()
                .take(MAX_WORDS)
                .collect(),
            lines,
            align: Align::ALL
                .into_iter()
                .find(|a| a.key() == key("align"))
                .unwrap_or_default(),
            border: Border::ALL
                .into_iter()
                .find(|b| b.key() == key("border"))
                .unwrap_or_default(),
            color: [channel(0)?, channel(1)?, channel(2)?],
            fill: json.get("fill").and_then(Json::as_bool).unwrap_or(false),
        })
    }

    /// The questions its `{ask:…}` fields ask, once each, in order.
    #[must_use]
    pub fn asks(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for line in &self.lines {
            for field in fields_in(&line.text) {
                if let Some(question) = field.strip_prefix("ask:") {
                    let question = question.trim().to_owned();
                    if !question.is_empty() && !out.contains(&question) {
                        out.push(question);
                    }
                }
            }
        }
        out
    }
}

/// The fields in `text`, without their braces.
fn fields_in(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// What fields are filled in with when a stamp is placed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    /// Who places it.
    pub name: String,
    /// When: year, month, day, hour, minute.
    pub when: (i32, u32, u32, u32, u32),
    /// Dates as 11/22/2026 when true, 2026-11-22 otherwise, unless a
    /// field asks for its own way.
    pub month_first: bool,
    /// The page, from 1.
    pub page: usize,
    /// The sheet's name: the page's label, or its number.
    pub sheet: String,
    /// The PDF's file name.
    pub file: String,
    /// The answers to `{ask:…}` fields, by question.
    pub asks: BTreeMap<String, String>,
}

impl Fields {
    /// `text` with its fields filled in; a field it does not know stays as
    /// written.
    #[must_use]
    pub fn fill(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                out.push_str(&rest[open..]);
                return out;
            };
            let field = &after[..close];
            if let Some(value) = self.value(field) {
                out.push_str(&value);
            } else {
                out.push('{');
                out.push_str(field);
                out.push('}');
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    fn value(&self, field: &str) -> Option<String> {
        let (y, mo, d, h, mi) = self.when;
        Some(match field.trim() {
            "name" => self.name.clone(),
            "date" if self.month_first => format!("{mo:02}/{d:02}/{y}"),
            "date" => format!("{y}-{mo:02}-{d:02}"),
            "time" => format!("{h:02}:{mi:02}"),
            "page" => self.page.to_string(),
            "sheet" => self.sheet.clone(),
            "file" => self.file.clone(),
            other => {
                if let Some(format) = other.strip_prefix("date:") {
                    return Some(date_as(format, self.when));
                }
                let question = other.strip_prefix("ask:")?.trim();
                self.asks.get(question).cloned().unwrap_or_default()
            }
        })
    }
}

/// The date `when` written as `format` says, with `yyyy`, `yy`, `MM`,
/// `dd`, `HH` and `mm` for year, month, day, hour and minute.
fn date_as(format: &str, (y, mo, d, h, mi): (i32, u32, u32, u32, u32)) -> String {
    let mut out = String::new();
    let mut rest = format;
    while !rest.is_empty() {
        let (piece, len) = if rest.starts_with("yyyy") {
            (format!("{y:04}"), 4)
        } else if rest.starts_with("yy") {
            (format!("{:02}", y.rem_euclid(100)), 2)
        } else if rest.starts_with("MM") {
            (format!("{mo:02}"), 2)
        } else if rest.starts_with("dd") {
            (format!("{d:02}"), 2)
        } else if rest.starts_with("HH") {
            (format!("{h:02}"), 2)
        } else if rest.starts_with("mm") {
            (format!("{mi:02}"), 2)
        } else {
            let c = rest.chars().next().unwrap_or_default();
            (c.to_string(), c.len_utf8())
        };
        out.push_str(&piece);
        rest = &rest[len..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Fields {
        Fields {
            name: "Andrés".into(),
            when: (2026, 11, 23, 9, 5),
            month_first: true,
            page: 3,
            sheet: "A-201".into(),
            file: "Kitchen.pdf".into(),
            asks: BTreeMap::from([("Job number".into(), "26-114".into())]),
        }
    }

    #[test]
    fn fields_are_filled_in_and_unknown_ones_stay() {
        let f = fields();
        assert_eq!(
            f.fill("{name} {date} {time} p{page} {sheet} {file}"),
            "Andrés 11/23/2026 09:05 p3 A-201 Kitchen.pdf"
        );
        assert_eq!(f.fill("{date:dd/MM/yyyy}"), "23/11/2026");
        assert_eq!(f.fill("{date:yy.MM.dd HH:mm}"), "26.11.23 09:05");
        assert_eq!(f.fill("JOB {ask:Job number}"), "JOB 26-114");
        assert_eq!(f.fill("{ask:Unanswered}!"), "!");
        assert_eq!(f.fill("{nobody} {open"), "{nobody} {open");
        let metric = Fields {
            month_first: false,
            ..fields()
        };
        assert_eq!(metric.fill("{date}"), "2026-11-23");
    }

    #[test]
    fn a_design_is_kept_in_a_file_and_with_its_stamp() {
        let mut design = Design::default();
        design.lines.push(Line {
            text: "JOB {ask:Job number} · {ask:Job number}".into(),
            size: 8.0,
            bold: false,
        });
        design.align = Align::Right;
        design.border = Border::Square;
        assert_eq!(design.asks(), ["Job number"]);
        assert_eq!(Design::from_json(&design.to_json()), Some(design.clone()));
        let look = design.look();
        assert_eq!(look, "a=r;b=s;s=20,9,8;w=1,0,0");
        let back = Design::from_look(&design.text(), &look, design.color, design.fill).unwrap();
        assert_eq!(back.lines, design.lines);
        assert_eq!((back.align, back.border), (Align::Right, Border::Square));
        assert_eq!(Design::from_look("X", "nonsense", BLUE, false), None);
        let (w, h) = design.size_with(&design.text());
        assert!(w > 100.0 && h > 40.0, "{w} × {h}");
    }

    #[test]
    fn a_built_in_stamp_is_a_design_too() {
        let approved = Design::from_template(&TEMPLATES[0]);
        assert_eq!(approved.name, "APPROVED");
        assert_eq!(approved.lines.len(), 2);
        let reviewed = Design::from_template(&TEMPLATES[8]);
        assert_eq!(reviewed.lines.len(), 1, "it says who and when already");
    }
}
