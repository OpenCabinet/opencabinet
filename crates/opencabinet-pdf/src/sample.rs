// SPDX-FileCopyrightText: 2026 OpenCabinet contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain sheet sets of our own, for tests and examples: no drawing from
//! anyone else is needed to try Review.

use std::fmt::Write as _;

/// A PDF of `pages` sheets of `width × height` points, each with a border,
/// a grid of `lines` lines across and as many down, and its sheet number
/// in the corner.
#[must_use]
pub fn sheets(pages: usize, width: f64, height: f64, lines: usize) -> Vec<u8> {
    let mut kids = String::new();
    for i in 0..pages {
        let _ = write!(kids, "{} 0 R ", 4 + 2 * i);
    }
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>").into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    for i in 0..pages {
        let mut content = format!(
            "0 0 0 RG 2 w 18 18 {:.2} {:.2} re S 0.5 w\n",
            width - 36.0,
            height - 36.0
        );
        let steps = u32::try_from(lines + 1).unwrap_or(u32::MAX);
        for n in 1..steps {
            let x = 18.0 + (width - 36.0) * f64::from(n) / f64::from(steps);
            let y = 18.0 + (height - 36.0) * f64::from(n) / f64::from(steps);
            let _ = writeln!(content, "{x:.2} 18 m {x:.2} {:.2} l S", height - 18.0);
            let _ = writeln!(content, "18 {y:.2} m {:.2} {y:.2} l S", width - 18.0);
        }
        let _ = writeln!(
            content,
            "BT /F1 14 Tf {:.2} 30 Td (A-{}) Tj ET",
            width - 90.0,
            201 + i
        );
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + 2 * i
            )
            .into_bytes(),
        );
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(table, "{offset:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.extend_from_slice(table.as_bytes());
    pdf
}
