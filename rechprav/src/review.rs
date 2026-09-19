//! KEEP review rendering. Wrapping uses shared lexical spans on both sides.
use crate::text::{Line, Tokens, tokenize};
use anyhow::{Result, ensure};
use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn fits(text: &str, width: usize) -> bool {
    text.chars().count() <= width && text.width() <= width
}

fn visible(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        match c {
            '\t' => result.push_str("\\t"),
            '\r' => result.push_str("\\r"),
            '\n' => result.push_str("\\n"),
            // Escape bidi controls, BOM, line separators, and terminal controls.
            c if c.is_control()
                || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}') =>
            {
                result.push_str(&format!("\\u{{{:x}}}", c as u32));
            }
            _ => result.push(c),
        }
    }
    result
}

fn split_display(text: &str, width: usize) -> Vec<String> {
    // One pathological combining cluster can itself exceed the scalar limit.
    let mut atoms = Vec::new();
    for g in text.graphemes(true) {
        if fits(g, width) {
            atoms.push(g.to_owned());
        } else {
            atoms.extend(g.chars().map(|c| format!("\\u{{{:x}}}", c as u32)));
        }
    }
    let mut parts = Vec::new();
    let mut part = String::new();
    for atom in atoms {
        if !fits(&format!("{part}{atom}"), width) && !part.is_empty() {
            parts.push(std::mem::take(&mut part));
        }
        part.push_str(&atom);
    }
    if !part.is_empty() || parts.is_empty() {
        parts.push(part);
    }
    parts
}

#[derive(Debug, Serialize)]
pub struct Segment {
    pub source: String,
    pub changes: String,
    pub token_start: usize,
    pub token_end: usize,
    /// Original unit range, repeated on continuation fragments (not a display offset).
    pub source_byte_start: usize,
    pub source_byte_end: usize,
    pub continuation_part: usize,
}

fn segments(source: &str, tokens: &Tokens, width: usize) -> Vec<Segment> {
    let mut units = Vec::new();
    if tokens.lexical.is_empty() {
        units.push((0, source.len(), 0, 0));
    } else {
        for i in 0..tokens.lexical.len() {
            let start = if i == 0 {
                0
            } else {
                tokens.lexical[i].byte_start
            };
            let end = tokens
                .lexical
                .get(i + 1)
                .map_or(source.len(), |t| t.byte_start);
            units.push((start, end, i, i + 1));
        }
    }
    let mut result = Vec::new();
    let mut current: Option<Segment> = None;
    for (start, end, token_start, token_end) in units {
        let display = visible(&source[start..end]);
        let marker = if token_start < token_end { "-" } else { "" };
        if !fits(&display, width) {
            if let Some(previous) = current.take() {
                result.push(previous);
            }
            for (part, fragment) in split_display(&display, width).into_iter().enumerate() {
                result.push(Segment {
                    source: fragment,
                    changes: if part == 0 {
                        marker.into()
                    } else {
                        "↳".into()
                    },
                    token_start,
                    token_end,
                    source_byte_start: start,
                    source_byte_end: end,
                    continuation_part: part,
                });
            }
            continue;
        }
        if let Some(previous) = current.as_ref() {
            let proposed_source = format!("{}{display}", previous.source);
            let proposed_changes = format!("{} {marker}", previous.changes);
            if !fits(&proposed_source, width) || !fits(&proposed_changes, width) {
                result.push(current.take().unwrap());
            }
        }
        if let Some(previous) = current.as_mut() {
            previous.source.push_str(&display);
            if !marker.is_empty() {
                if !previous.changes.is_empty() {
                    previous.changes.push(' ');
                }
                previous.changes.push_str(marker);
            }
            previous.token_end = token_end;
            previous.source_byte_end = end;
        } else {
            current = Some(Segment {
                source: display,
                changes: marker.into(),
                token_start,
                token_end,
                source_byte_start: start,
                source_byte_end: end,
                continuation_part: 0,
            });
        }
    }
    if let Some(previous) = current {
        result.push(previous);
    }
    result
}

#[derive(Debug, Serialize)]
pub struct Decision<'a> {
    pub token: usize,
    pub original: &'a str,
    pub candidate: &'a str,
    pub confidence: Option<f64>,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DebugLine<'a> {
    pub schema_version: u32,
    pub engine: &'static str,
    pub line: usize,
    pub file_byte_start: usize,
    pub source: &'a str,
    pub ending: &'a str,
    pub strict_yo: bool,
    pub tokens: Tokens,
    pub decisions: Vec<Decision<'a>>,
    pub segments: Vec<Segment>,
}

pub fn render<'a>(
    line: &Line<'a>,
    width: usize,
    strict_yo: bool,
) -> Result<(String, DebugLine<'a>)> {
    ensure!(
        (32..=130).contains(&width),
        "width must be between 32 and 130"
    );
    let tokens = tokenize(line.content, strict_yo);
    let segments = segments(line.content, &tokens, width);
    let mut review = String::new();
    for (i, segment) in segments.iter().enumerate() {
        review.push_str(&format!(
            "[{:06}.{}]\n{}\n{}\n\n",
            line.number,
            i + 1,
            segment.source,
            segment.changes
        ));
    }
    let decisions = tokens
        .lexical
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let original = &line.content[t.byte_start..t.byte_end];
            Decision {
                token: i,
                original,
                candidate: original,
                confidence: None,
                reason: "keep_baseline",
            }
        })
        .collect();
    Ok((
        review,
        DebugLine {
            schema_version: 1,
            engine: "keep",
            line: line.number,
            file_byte_start: line.byte_start,
            source: line.content,
            ending: line.ending,
            strict_yo,
            tokens,
            decisions,
            segments,
        },
    ))
}
