//! Review rendering. Wrapping uses shared lexical spans on both sides.
use crate::{
    corpus::Edit,
    corrector::{Corrector, Decision, Keep, analyze},
    text::{Line, Tokens},
};
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

fn segments(source: &str, tokens: &Tokens, decisions: &[Decision], width: usize) -> Vec<Segment> {
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
        let marker = if token_start < token_end {
            if decisions[token_start].changed() {
                decisions[token_start].candidate.as_str()
            } else {
                "-"
            }
        } else {
            ""
        };
        if !fits(&display, width) || !fits(marker, width) {
            if let Some(previous) = current.take() {
                result.push(previous);
            }
            let source_parts = split_display(&display, width);
            let change_parts = split_display(marker, width);
            for part in 0..source_parts.len().max(change_parts.len()) {
                result.push(Segment {
                    source: source_parts.get(part).cloned().unwrap_or_default(),
                    changes: change_parts
                        .get(part)
                        .cloned()
                        .unwrap_or_else(|| "↳".into()),
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
pub struct DebugLine<'a> {
    pub schema_version: u32,
    pub engine: &'static str,
    pub line: usize,
    pub file_byte_start: usize,
    pub source: &'a str,
    pub ending: &'a str,
    pub strict_yo: bool,
    pub tokens: Tokens,
    pub decisions: Vec<Decision>,
    pub corrected: String,
    pub edits: Vec<Edit>,
    pub segments: Vec<Segment>,
}

pub fn render<'a>(
    line: &Line<'a>,
    width: usize,
    strict_yo: bool,
) -> Result<(String, DebugLine<'a>)> {
    render_engine(line, width, &Keep { strict_yo })
}

pub fn render_engine<'a>(
    line: &Line<'a>,
    width: usize,
    engine: &dyn Corrector,
) -> Result<(String, DebugLine<'a>)> {
    ensure!(
        (32..=130).contains(&width),
        "width must be between 32 and 130"
    );
    let analysis = analyze(line.content, engine);
    let segments = segments(line.content, &analysis.tokens, &analysis.decisions, width);
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
    Ok((
        review,
        DebugLine {
            schema_version: 2,
            engine: engine.name(),
            line: line.number,
            file_byte_start: line.byte_start,
            source: line.content,
            ending: line.ending,
            strict_yo: engine.strict_yo(),
            tokens: analysis.tokens,
            decisions: analysis.decisions,
            corrected: analysis.corrected,
            edits: analysis.edits,
            segments,
        },
    ))
}
