//! Source offsets always refer to original UTF-8 bytes, never normalized keys.
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Word,
    Number,
    Protected,
    Punctuation,
    Whitespace,
}

#[derive(Debug, Serialize)]
pub struct TextToken {
    pub byte_start: usize,
    pub byte_end: usize,
    pub kind: Kind,
    pub lexical_index: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct LexicalToken {
    pub text_token: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub key: String,
    pub protection: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct Tokens {
    pub text: Vec<TextToken>,
    pub lexical: Vec<LexicalToken>,
}

pub fn key(text: &str, strict_yo: bool) -> String {
    let normalized: String = text.nfc().flat_map(char::to_lowercase).nfc().collect();
    if strict_yo {
        normalized
    } else {
        normalized.replace('ё', "е")
    }
}

// Patterns are anchored: unrecognized punctuation never swallows the next word.
static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)^(?:https?://|www\.)[^\s<>"«»]+"#).unwrap());
static PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^(?:[A-Za-z]:[\\/]|\\\\|~/|\./|\.\./|/)[^\s<>"«»]+"#).unwrap());
static RELATIVE_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^[\p{L}\p{N}_.-]+(?:[/\\][^\s<>"«»]+)+"#).unwrap());
static EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}._%+-]+@[\p{L}\p{N}.-]+\.[\p{L}]{2,}").unwrap());
static SPECIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[Cc]\+\+|[Cc]#|--?[\p{L}][\p{L}\p{N}_-]*(?:=[^\s]+)?)").unwrap()
});
static ABBREV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:[\p{L}]{1,3}\.){2,}").unwrap());
// Protect filenames/dotted identifiers and inline code before enabling correction.
// A missing closing backtick protects the remainder of that physical line.
static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^`+[^`\r\n]*(?:`+|$)").unwrap());
static DOTTED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_-]+(?:\.[\p{L}\p{N}_-]+)+").unwrap());
static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\p{N}+(?:[.,:/-]\p{N}+)*(?:[%‰])?").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{L}\p{N}][\p{L}\p{M}\p{N}_]*(?:[-‐‑'’][\p{L}\p{N}][\p{L}\p{M}\p{N}_]*)*")
        .unwrap()
});
static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+").unwrap());

fn trim_endpoint(value: &str) -> usize {
    let mut end = value.len();
    loop {
        let part = &value[..end];
        let Some(c) = part.chars().last() else { break };
        let unmatched = [(')', '('), (']', '['), ('}', '{')]
            .iter()
            .any(|&(close, open)| {
                c == close && part.matches(close).count() > part.matches(open).count()
            });
        if ".,;:!?".contains(c) || unmatched {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    end
}

pub fn tokenize(source: &str, strict_yo: bool) -> Tokens {
    let mut result = Tokens {
        text: vec![],
        lexical: vec![],
    };
    let mut start = 0;
    while start < source.len() {
        let rest = &source[start..];
        let mut found = None;
        for (pattern, kind, trim) in [
            (&*SPACE, Kind::Whitespace, false),
            (&*CODE, Kind::Protected, false),
            (&*URL, Kind::Protected, true),
            (&*PATH, Kind::Protected, true),
            (&*RELATIVE_PATH, Kind::Protected, true),
            (&*EMAIL, Kind::Protected, false),
            (&*SPECIAL, Kind::Protected, false),
            (&*ABBREV, Kind::Protected, false),
            (&*DOTTED, Kind::Protected, false),
        ] {
            if let Some(m) = pattern.find(rest) {
                let len = if trim {
                    trim_endpoint(m.as_str())
                } else {
                    m.end()
                };
                if len > 0 {
                    found = Some((len, kind));
                    break;
                }
            }
        }
        // Take a whole identifier rather than splitting 4090RTX into number + word.
        let (len, kind) = found.unwrap_or_else(|| {
            let word = WORD.find(rest).map_or(0, |m| m.end());
            let number = NUMBER.find(rest).map_or(0, |m| m.end());
            if number > 0 && number >= word {
                (number, Kind::Number)
            } else if word > 0 {
                (word, Kind::Word)
            } else {
                (
                    rest.graphemes(true).next().unwrap().len(),
                    Kind::Punctuation,
                )
            }
        });
        let end = start + len;
        let value = &source[start..end];
        let kind = if kind == Kind::Protected
            && NUMBER.find(value).is_some_and(|m| m.end() == value.len())
        {
            Kind::Number
        } else {
            kind
        };
        let lexical_index = if matches!(kind, Kind::Word | Kind::Number | Kind::Protected) {
            let normalized = key(value, strict_yo);
            let is_russian_word = normalized
                .chars()
                .all(|c| ('а'..='я').contains(&c) || c == 'ё');
            let uppercase = value.chars().filter(|c| c.is_uppercase()).count();
            let title_case = uppercase == 1 && value.chars().next().is_some_and(char::is_uppercase);
            let protection = if kind == Kind::Number {
                Some("number")
            } else if kind == Kind::Protected {
                Some("technical_atom")
            } else if !is_russian_word {
                Some("non_simple_russian")
            } else if uppercase > 0 && !title_case {
                Some("uppercase_or_mixed_case")
            } else if normalized.chars().count() <= 3 {
                Some("short_word")
            } else {
                None
            };
            let index = result.lexical.len();
            result.lexical.push(LexicalToken {
                text_token: result.text.len(),
                byte_start: start,
                byte_end: end,
                key: normalized,
                protection,
            });
            Some(index)
        } else {
            None
        };
        result.text.push(TextToken {
            byte_start: start,
            byte_end: end,
            kind,
            lexical_index,
        });
        start = end;
    }
    result
}

#[derive(Debug)]
pub struct Line<'a> {
    pub number: usize,
    pub byte_start: usize,
    pub content: &'a str,
    pub ending: &'a str,
}

/// LF, CRLF and bare CR are preserved. A trailing delimiter isn't a phantom record.
pub fn lines(source: &str) -> Vec<Line<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    while start < source.len() {
        let end = source[start..]
            .find(['\r', '\n'])
            .map_or(source.len(), |i| start + i);
        let ending_len = if source[end..].starts_with("\r\n") {
            2
        } else if end < source.len() {
            1
        } else {
            0
        };
        result.push(Line {
            number: result.len() + 1,
            byte_start: start,
            content: &source[start..end],
            ending: &source[end..end + ending_len],
        });
        start = end + ending_len;
    }
    result
}
