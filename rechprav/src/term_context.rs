//! Adjacent lexical context templates; deliberately no regex, gaps or morphology.
use crate::text::{self, Kind, Tokens};
use anyhow::{Result, ensure};

pub(crate) fn horizontal(gap: &str) -> bool {
    !gap.is_empty() && gap.chars().all(|c| matches!(c, ' ' | '\t' | '\u{00a0}'))
}

pub(crate) struct Pattern {
    pub label: String,
    before: Vec<String>,
    after: Vec<String>,
}

impl Pattern {
    pub fn parse(value: &str, strict: bool, window: usize) -> Result<Self> {
        let (before, after) = value
            .split_once("{term}")
            .ok_or_else(|| anyhow::anyhow!("context pattern requires one {{term}} placeholder"))?;
        let left: Vec<_> = before.split_whitespace().collect();
        let right: Vec<_> = after.split_whitespace().collect();
        let canonical = left
            .iter()
            .copied()
            .chain(["{term}"])
            .chain(right.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        ensure!(
            canonical == value
                && !after.contains("{term}")
                && (!left.is_empty() || !right.is_empty())
                && left.len() <= window.min(4)
                && right.len() <= window.min(4)
                && left
                    .iter()
                    .chain(&right)
                    .all(|w| w.chars().count() <= 64 && w.chars().all(char::is_alphabetic)),
            "context pattern requires 1..4 adjacent alphabetic words per nonempty side, single spaces and one {{term}}"
        );
        Ok(Self {
            label: value.into(),
            before: left.into_iter().map(|w| text::key(w, strict)).collect(),
            after: right.into_iter().map(|w| text::key(w, strict)).collect(),
        })
    }

    pub fn matches(&self, source: &str, tokens: &Tokens, start: usize, end: usize) -> bool {
        if self.before.len() > start || end + self.after.len() > tokens.lexical.len() {
            return false;
        }
        let left = start - self.before.len();
        let right = end + self.after.len();
        for i in left + 1..right {
            if !horizontal(&source[tokens.lexical[i - 1].byte_end..tokens.lexical[i].byte_start]) {
                return false;
            }
        }
        self.before
            .iter()
            .zip(&tokens.lexical[left..start])
            .chain(self.after.iter().zip(&tokens.lexical[end..right]))
            .all(|(key, t)| *key == t.key && tokens.text[t.text_token].kind == Kind::Word)
    }
}
