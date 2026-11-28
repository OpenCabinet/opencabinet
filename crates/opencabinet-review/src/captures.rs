// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a capture markup keeps (see [`opencabinet_pdf::capture`]): which
//! part of which page it shows, the colors found there, and the colors
//! changed. It is kept in the markup's look, written short:
//! `p=0;r=10,20,300,400;c=000000,808080;m=000000>ff00ff`.

use opencabinet_pdf::capture::Rgb;

/// What a capture shows, and how its colors are changed.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureLook {
    /// The page it was taken from, from 0.
    pub page: usize,
    /// The part taken, in that page's PDF space: left, bottom, right, top.
    pub region: [f64; 4],
    /// The colors found there, the most used first.
    pub found: Vec<Rgb>,
    /// Colors changed, from and to.
    pub map: Vec<(Rgb, Rgb)>,
}

fn hex(c: Rgb) -> String {
    use std::fmt::Write as _;
    c.iter().fold(String::new(), |mut out, v| {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 to 255"
        )]
        let byte = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn unhex(text: &str) -> Option<Rgb> {
    if text.len() != 6 {
        return None;
    }
    let channel = |i: usize| -> Option<f64> {
        Some(f64::from(u8::from_str_radix(text.get(i..i + 2)?, 16).ok()?) / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

impl CaptureLook {
    /// Written short, to keep in a markup's look.
    #[must_use]
    pub fn look(&self) -> String {
        let [x0, y0, x1, y1] = self.region;
        let found: Vec<String> = self.found.iter().map(|&c| hex(c)).collect();
        let map: Vec<String> = self
            .map
            .iter()
            .map(|&(from, to)| format!("{}>{}", hex(from), hex(to)))
            .collect();
        format!(
            "p={};r={x0:.2},{y0:.2},{x1:.2},{y1:.2};c={};m={}",
            self.page,
            found.join(","),
            map.join(",")
        )
    }

    /// What a markup's look keeps; `None` when it is not a capture's.
    #[must_use]
    pub fn from_look(look: &str) -> Option<Self> {
        let part = |key: &str| {
            look.split(';')
                .find_map(|p| p.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
        };
        let region: Vec<f64> = part("r")?
            .split(',')
            .filter_map(|v| v.parse().ok())
            .collect();
        let [x0, y0, x1, y1] = region[..] else {
            return None;
        };
        Some(Self {
            page: part("p")?.parse().ok()?,
            region: [x0, y0, x1, y1],
            found: part("c")
                .unwrap_or_default()
                .split(',')
                .filter_map(unhex)
                .collect(),
            map: part("m")
                .unwrap_or_default()
                .split(',')
                .filter_map(|pair| {
                    let (from, to) = pair.split_once('>')?;
                    Some((unhex(from)?, unhex(to)?))
                })
                .collect(),
        })
    }

    /// What `color` becomes, if it is changed.
    #[must_use]
    pub fn changed(&self, color: Rgb) -> Option<Rgb> {
        self.map
            .iter()
            .find(|(from, _)| opencabinet_pdf::capture::same(*from, color))
            .map(|&(_, to)| to)
    }

    /// Makes `from` become `to`, or stay as it is with `None`.
    pub fn change(&mut self, from: Rgb, to: Option<Rgb>) {
        self.map
            .retain(|(f, _)| !opencabinet_pdf::capture::same(*f, from));
        if let Some(to) = to {
            self.map.push((from, to));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_capture_is_kept_in_its_look() {
        let mut capture = CaptureLook {
            page: 2,
            region: [10.0, 20.5, 300.0, 400.25],
            found: vec![[0.0; 3], [0.5; 3]],
            map: Vec::new(),
        };
        capture.change([0.0; 3], Some([1.0, 0.0, 1.0]));
        assert_eq!(capture.changed([0.0; 3]), Some([1.0, 0.0, 1.0]));
        let look = capture.look();
        assert_eq!(
            look,
            "p=2;r=10.00,20.50,300.00,400.25;c=000000,808080;m=000000>ff00ff"
        );
        let back = CaptureLook::from_look(&look).unwrap();
        assert_eq!(back.page, 2);
        assert_eq!(back.map.len(), 1);
        assert!((back.found[1][0] - 128.0 / 255.0).abs() < 1e-9);
        capture.change([0.0; 3], None);
        assert_eq!(capture.changed([0.0; 3]), None);
        assert_eq!(CaptureLook::from_look("a=c;b=r"), None);
    }
}
