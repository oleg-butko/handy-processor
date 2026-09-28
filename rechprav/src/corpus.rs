use crate::text::{key, lines, tokenize};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Unassigned,
    Train,
    Dev,
    Test,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Reviewed,
    Ambiguous,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Synthetic,
    RealAsr,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub kind: Origin,
    pub origin: String,
    pub asr_model: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub byte_start: usize,
    pub byte_end: usize,
    pub token_start: usize,
    pub token_end: usize,
    pub replacement: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub id: String,
    pub group_id: String,
    pub split: Split,
    pub source: String,
    pub gold: Option<String>,
    pub edits: Vec<Edit>,
    pub tags: Vec<String>,
    pub provenance: Provenance,
    pub annotation_status: Status,
}

pub fn read(path: &Path) -> Result<Vec<Record>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("read UTF-8 {}", path.display()))?;
    parse(&text, path)
}

/// Parse the same captured bytes that the experiment manifest hashes.
pub fn parse(text: &str, path: &Path) -> Result<Vec<Record>> {
    let records: Result<Vec<Record>> = text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            ensure!(
                !line.trim().is_empty(),
                "{}:{}: blank JSONL record",
                path.display(),
                i + 1
            );
            serde_json::from_str(line).with_context(|| format!("{}:{}", path.display(), i + 1))
        })
        .collect();
    let records = records?;
    ensure!(!records.is_empty(), "empty corpus: {}", path.display());
    Ok(records)
}

pub fn validate_record(record: &Record) -> Result<()> {
    ensure!(record.schema_version == 1, "unsupported schema_version");
    ensure!(
        !record.id.trim().is_empty() && !record.group_id.trim().is_empty(),
        "empty id/group_id"
    );
    ensure!(
        !record.provenance.origin.trim().is_empty(),
        "empty provenance.origin"
    );
    if record.annotation_status != Status::Reviewed {
        ensure!(
            record.gold.is_none() && record.edits.is_empty(),
            "pending/ambiguous records require gold=null and edits=[]"
        );
        return Ok(());
    }
    let gold = record
        .gold
        .as_deref()
        .context("reviewed record requires gold")?;
    let tokens = tokenize(&record.source, false);
    let mut cursor = 0;
    let mut result = String::new();
    for edit in &record.edits {
        ensure!(
            edit.byte_start >= cursor
                && edit.byte_start < edit.byte_end
                && edit.byte_end <= record.source.len(),
            "unordered, overlapping or invalid byte range"
        );
        ensure!(
            record.source.is_char_boundary(edit.byte_start)
                && record.source.is_char_boundary(edit.byte_end),
            "range splits UTF-8 character"
        );
        ensure!(
            edit.token_start < edit.token_end && edit.token_end <= tokens.lexical.len(),
            "invalid lexical range"
        );
        ensure!(
            tokens.lexical[edit.token_start].byte_start == edit.byte_start
                && tokens.lexical[edit.token_end - 1].byte_end == edit.byte_end,
            "byte/lexical ranges disagree"
        );
        ensure!(
            !tokens.text.iter().any(|t| t.byte_start >= edit.byte_start
                && t.byte_end <= edit.byte_end
                && t.kind == crate::text::Kind::Punctuation),
            "lexical edit cannot remove punctuation"
        );
        ensure!(
            record.source[edit.byte_start..edit.byte_end] != edit.replacement,
            "no-op edit"
        );
        ensure!(
            !edit.replacement.contains(['\r', '\n']),
            "replacement cannot introduce line breaks"
        );
        // This schema covers lexical substitutions/compounds, not arbitrary text rewriting.
        let replacement = tokenize(&edit.replacement, false);
        ensure!(
            !replacement
                .text
                .iter()
                .any(|t| t.kind == crate::text::Kind::Punctuation),
            "lexical edit cannot insert punctuation"
        );
        ensure!(
            !replacement.lexical.is_empty(),
            "empty lexical replacement is not supported in schema 1"
        );
        ensure!(
            replacement.lexical[0].byte_start == 0
                && replacement.lexical.last().unwrap().byte_end == edit.replacement.len(),
            "replacement must start and end with lexical tokens"
        );
        ensure!(
            !record.source[edit.byte_start..edit.byte_end].contains(['\r', '\n']),
            "edit crosses a line boundary"
        );
        result.push_str(&record.source[cursor..edit.byte_start]);
        result.push_str(&edit.replacement);
        cursor = edit.byte_end;
    }
    result.push_str(&record.source[cursor..]);
    ensure!(
        result == gold,
        "applying edits does not reproduce gold exactly"
    );
    Ok(())
}

/// The same key is used for leakage checks and safe grouping before a split.
pub fn leakage_key(phrase: &str) -> String {
    let normalized = tokenize(phrase, false)
        .lexical
        .into_iter()
        .map(|token| token.key)
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        key(phrase, false)
    } else {
        normalized
    }
}

#[derive(Debug, Serialize)]
pub struct Validation {
    pub records: usize,
    pub reviewed: usize,
    pub pending: usize,
    pub ambiguous: usize,
    pub synthetic: usize,
    pub real_asr: usize,
    pub unassigned: usize,
}

/// Checks all supplied splits together. Duplicate source/gold keys share one namespace.
pub fn validate(records: &[Record]) -> Result<Validation> {
    ensure!(!records.is_empty(), "empty corpus");
    let mut ids = BTreeMap::new();
    let mut groups: BTreeMap<&str, Split> = BTreeMap::new();
    let mut phrases: BTreeMap<String, Split> = BTreeMap::new();
    let mut summary = Validation {
        records: records.len(),
        reviewed: 0,
        pending: 0,
        ambiguous: 0,
        synthetic: 0,
        real_asr: 0,
        unassigned: 0,
    };
    for record in records {
        validate_record(record).with_context(|| format!("record {}", record.id))?;
        ensure!(
            ids.insert(&record.id, ()).is_none(),
            "duplicate id: {}",
            record.id
        );
        if let Some(previous) = groups.insert(record.group_id.as_str(), record.split) {
            ensure!(
                previous == record.split,
                "group spans splits: {}",
                record.id
            );
        }
        for phrase in std::iter::once(record.source.as_str()).chain(record.gold.as_deref()) {
            let normalized = leakage_key(phrase);
            if let Some(previous) = phrases.insert(normalized, record.split) {
                ensure!(
                    previous == record.split,
                    "normalized source/gold overlap between splits: {}",
                    record.id
                );
            }
        }
        match record.annotation_status {
            Status::Reviewed => summary.reviewed += 1,
            Status::Pending => summary.pending += 1,
            Status::Ambiguous => summary.ambiguous += 1,
        }
        match record.provenance.kind {
            Origin::Synthetic => summary.synthetic += 1,
            Origin::RealAsr => summary.real_asr += 1,
        }
        if record.split == Split::Unassigned {
            summary.unassigned += 1;
        }
    }
    Ok(summary)
}

pub fn prepare(source: &str, group: &str, origin: Origin) -> Result<Vec<Record>> {
    ensure!(!group.trim().is_empty(), "group must be nonempty");
    // The Handy legacy export is a source/result report, not a plain line corpus.
    if source
        .lines()
        .any(|line| line == "--- РЕЗУЛЬТАТ ---" || line == "==============================")
    {
        bail!(
            "legacy Handy export is ambiguous; provide source-only lines or structured JSONL, never use transformed text as gold"
        );
    }
    let records = lines(source)
        .into_iter()
        .map(|line| Record {
            schema_version: 1,
            id: format!("{group}:{:06}", line.number),
            group_id: group.into(),
            split: Split::Unassigned,
            source: line.content.into(),
            gold: None,
            edits: vec![],
            tags: vec![],
            provenance: Provenance {
                kind: origin,
                origin: format!("plain-lines:{}", line.number),
                asr_model: None,
            },
            annotation_status: Status::Pending,
        })
        .collect::<Vec<_>>();
    validate(&records)?;
    Ok(records)
}
