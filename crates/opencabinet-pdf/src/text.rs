// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text in PDF strings: text strings in dictionaries (`PDFDocEncoding` or
//! UTF-16), and text drawn with a standard font (`WinAnsiEncoding`).

/// A text string for a dictionary: plain bytes when every character is
/// ASCII, otherwise UTF-16 big endian after its byte order mark.
pub(crate) fn encode(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        return text.as_bytes().to_vec();
    }
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

/// A text string read from a dictionary.
pub(crate) fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_be_bytes(pair))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    // PDFDocEncoding matches Latin-1 for the characters drawings use.
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// `text` as a literal string to show with a standard font in
/// `WinAnsiEncoding`, parentheses included. Characters the encoding lacks
/// become `?`.
#[must_use]
pub fn win_ansi_literal(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        let byte = match c {
            '\u{20}'..='\u{7e}' | '\u{a0}'..='\u{ff}' => u8::try_from(u32::from(c)).unwrap_or(b'?'),
            '€' => 0x80,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        };
        if matches!(byte, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(byte);
    }
    out.push(b')');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_strings_come_back_as_written() {
        for text in ["Andrés", "Confirm 36\" height", "", "½ — ok"] {
            assert_eq!(decode(&encode(text)), text);
        }
        assert_eq!(encode("plain"), b"plain");
    }

    #[test]
    fn shown_text_is_escaped_and_encoded() {
        assert_eq!(win_ansi_literal("a (b) \\ é"), b"(a \\(b\\) \\\\ \xe9)");
        assert_eq!(win_ansi_literal("漢"), b"(?)");
    }
}
