//! Convert an explicitly reviewed browser project into the existing corpus schema.
//! Unsupported edits are reported, never silently rewritten into a different gold.
use crate::{corpus, hashing, text};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
struct ProjectLine {
    schema_version: u32,
    history: History,
    annotation: Annotation,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct History {
    id: u64,
    original_text: String,
    #[serde(rename = "source")]
    source_kind: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum OriginKind {
    Unknown,
    RealAsr,
    Synthetic,
    NotAsr,
}

impl Default for OriginKind {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Annotation {
    status: corpus::Status,
    gold: Option<String>,
    #[serde(default)]
    origin_kind: OriginKind,
    #[serde(default)]
    group_id: String,
}

#[derive(Debug, Serialize)]
pub struct Exclusion {
    pub history_id: u64,
    pub reason: &'static str,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub project_sha256: String,
    pub input_records: usize,
    pub exported_records: usize,
    pub real_asr_records: usize,
    pub synthetic_records: usize,
    pub groups: usize,
    pub exclusions: Vec<Exclusion>,
}

pub struct Conversion {
    pub records: Vec<corpus::Record>,
    pub report: Report,
}

fn token_text<'a>(source: &'a str, token: &text::LexicalToken) -> &'a str {
    &source[token.byte_start..token.byte_end]
}

/// LCS anchors exact lexical tokens; the corpus validator checks every untouched byte.
fn derive_edits(source: &str, gold: &str) -> Result<Vec<corpus::Edit>> {
    if source == gold {
        return Ok(vec![]);
    }
    let input = text::tokenize(source, false);
    let output = text::tokenize(gold, false);
    let n = input.lexical.len();
    let m = output.lexical.len();
    ensure!(n <= 256 && m <= 256, "more than 256 lexical tokens");
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] =
                if token_text(source, &input.lexical[i]) == token_text(gold, &output.lexical[j]) {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
        }
    }
    let mut anchors = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if token_text(source, &input.lexical[i]) == token_text(gold, &output.lexical[j]) {
            anchors.push((i, j));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    anchors.push((n, m));

    let (mut input_start, mut output_start) = (0, 0);
    let mut edits = Vec::new();
    for (next_input, next_output) in anchors {
        if input_start != next_input || output_start != next_output {
            ensure!(
                input_start < next_input && output_start < next_output,
                "lexical insertion or deletion needs manual review"
            );
            let first = &input.lexical[input_start];
            let last = &input.lexical[next_input - 1];
            let replacement_start = output.lexical[output_start].byte_start;
            let replacement_end = output.lexical[next_output - 1].byte_end;
            edits.push(corpus::Edit {
                byte_start: first.byte_start,
                byte_end: last.byte_end,
                token_start: input_start,
                token_end: next_input,
                replacement: gold[replacement_start..replacement_end].to_string(),
            });
        }
        input_start = next_input + 1;
        output_start = next_output + 1;
    }
    Ok(edits)
}

fn find(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        parent[index] = find(parent, parent[index]);
    }
    parent[index]
}

fn union(parent: &mut [usize], left: usize, right: usize) {
    let a = find(parent, left);
    let b = find(parent, right);
    if a != b {
        parent[b] = a;
    }
}

pub fn convert(contents: &str) -> Result<Conversion> {
    let mut report = Report {
        project_sha256: hashing::sha256_hex(contents.as_bytes()),
        input_records: 0,
        exported_records: 0,
        real_asr_records: 0,
        synthetic_records: 0,
        groups: 0,
        exclusions: Vec::new(),
    };
    let mut records = Vec::new();
    let mut manual_groups = Vec::new();
    let mut ids = HashSet::new();
    for (line_number, line) in contents.lines().enumerate() {
        ensure!(
            !line.trim().is_empty(),
            "blank project line {}",
            line_number + 1
        );
        let mut item: ProjectLine = serde_json::from_str(line)
            .with_context(|| format!("project line {}", line_number + 1))?;
        ensure!(
            item.schema_version == 1 || item.schema_version == 2,
            "unsupported project schema on line {}",
            line_number + 1
        );
        if item.schema_version == 1 {
            item.annotation.origin_kind = OriginKind::Unknown;
            item.annotation.group_id.clear();
        }
        ensure!(
            item.history.id > 0,
            "invalid history id on line {}",
            line_number + 1
        );
        ensure!(
            ids.insert(item.history.id),
            "duplicate history id {}",
            item.history.id
        );
        ensure!(
            item.history.source_kind == "external" || item.history.source_kind == "voice-editor",
            "invalid history source on line {}",
            line_number + 1
        );
        ensure!(
            item.annotation.group_id.len() <= 120,
            "group name too long on line {}",
            line_number + 1
        );
        ensure!(
            (item.annotation.status == corpus::Status::Reviewed) == item.annotation.gold.is_some(),
            "annotation status/gold disagree on line {}",
            line_number + 1
        );
        report.input_records += 1;
        let reason = match item.annotation.status {
            corpus::Status::Pending => Some("pending"),
            corpus::Status::Ambiguous => Some("ambiguous"),
            corpus::Status::Reviewed => match item.annotation.origin_kind {
                OriginKind::Unknown => Some("origin_unconfirmed"),
                OriginKind::NotAsr => Some("not_asr"),
                OriginKind::RealAsr | OriginKind::Synthetic => None,
            },
        };
        if let Some(reason) = reason {
            report.exclusions.push(Exclusion {
                history_id: item.history.id,
                reason,
                detail: None,
            });
            continue;
        }
        let origin = match item.annotation.origin_kind {
            OriginKind::RealAsr => corpus::Origin::RealAsr,
            OriginKind::Synthetic => corpus::Origin::Synthetic,
            _ => unreachable!(),
        };
        let source = item.history.original_text;
        let gold = item.annotation.gold.unwrap();
        let edits = match derive_edits(&source, &gold) {
            Ok(edits) => edits,
            Err(error) => {
                report.exclusions.push(Exclusion {
                    history_id: item.history.id,
                    reason: "unsupported_edit",
                    detail: Some(error.to_string()),
                });
                continue;
            }
        };
        let candidate = corpus::Record {
            schema_version: 1,
            id: format!("history:{}", item.history.id),
            group_id: format!("history-group:{}", item.history.id),
            split: corpus::Split::Unassigned,
            source,
            gold: Some(gold),
            edits,
            tags: vec![format!("history-source:{}", item.history.source_kind)],
            provenance: corpus::Provenance {
                kind: origin,
                origin: format!("handy-history:{}", item.history.id),
                asr_model: None,
            },
            annotation_status: corpus::Status::Reviewed,
        };
        if let Err(error) = corpus::validate_record(&candidate) {
            report.exclusions.push(Exclusion {
                history_id: item.history.id,
                reason: "unsupported_edit",
                detail: Some(error.to_string()),
            });
            continue;
        }
        manual_groups.push(item.annotation.group_id.trim().to_string());
        records.push(candidate);
    }
    if report.input_records == 0 {
        bail!("empty annotation project");
    }
    let mut parent = (0..records.len()).collect::<Vec<_>>();
    let mut phrase_owner = HashMap::new();
    let mut manual_owner = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        for key in [
            corpus::leakage_key(&record.source),
            corpus::leakage_key(record.gold.as_deref().unwrap()),
        ] {
            if let Some(&other) = phrase_owner.get(&key) {
                union(&mut parent, index, other);
            } else {
                phrase_owner.insert(key, index);
            }
        }
        if !manual_groups[index].is_empty() {
            if let Some(&other) = manual_owner.get(&manual_groups[index]) {
                union(&mut parent, index, other);
            } else {
                manual_owner.insert(manual_groups[index].clone(), index);
            }
        }
    }
    let mut component_min = HashMap::new();
    for index in 0..records.len() {
        let root = find(&mut parent, index);
        let id = records[index]
            .id
            .strip_prefix("history:")
            .unwrap()
            .parse::<u64>()?;
        component_min
            .entry(root)
            .and_modify(|minimum: &mut u64| *minimum = (*minimum).min(id))
            .or_insert(id);
    }
    for index in 0..records.len() {
        let root = find(&mut parent, index);
        records[index].group_id = format!("history-group:{}", component_min[&root]);
    }
    report.groups = component_min.len();
    report.exported_records = records.len();
    report.real_asr_records = records
        .iter()
        .filter(|record| record.provenance.kind == corpus::Origin::RealAsr)
        .count();
    report.synthetic_records = records.len() - report.real_asr_records;
    if !records.is_empty() {
        corpus::validate(&records)?;
    }
    Ok(Conversion { records, report })
}

#[cfg(test)]
mod tests {
    use super::convert;
    use serde_json::json;

    fn item(
        id: u64,
        source: &str,
        gold: Option<&str>,
        status: &str,
        origin: &str,
        group: &str,
    ) -> String {
        json!({
            "schema_version": 2,
            "history": {"id":id,"originalText":source,"source":"external"},
            "annotation": {"status":status,"gold":gold,"originKind":origin,"groupId":group}
        })
        .to_string()
    }

    #[test]
    fn derives_multiple_utf8_edits_and_groups_related_phrases() {
        let input = [
            item(
                1,
                "свуты и пнзэ",
                Some("слот и пнз"),
                "reviewed",
                "synthetic",
                "slot",
            ),
            item(
                2,
                "сняли свод",
                Some("сняли слот"),
                "reviewed",
                "synthetic",
                "slot",
            ),
            item(
                3,
                "сняли свод",
                Some("сняли свод"),
                "reviewed",
                "synthetic",
                "",
            ),
        ]
        .join("\n");
        let conversion = convert(&input).unwrap();
        assert_eq!(conversion.records.len(), 3);
        assert_eq!(conversion.records[0].edits.len(), 2);
        assert_eq!(conversion.records[0].edits[0].byte_end, "свуты".len());
        assert!(
            conversion
                .records
                .iter()
                .all(|record| record.group_id == "history-group:1")
        );
        assert_eq!(conversion.report.groups, 1);
    }

    #[test]
    fn excludes_unconfirmed_and_unsupported_without_changing_gold() {
        let input = [
            item(1, "свод", Some("слот"), "reviewed", "unknown", ""),
            item(2, "свод", Some("слот"), "reviewed", "real-asr", ""),
            item(
                3,
                "нет, звон",
                Some("недозвон"),
                "reviewed",
                "synthetic",
                "",
            ),
            item(4, "всё верно", None, "ambiguous", "real-asr", ""),
        ]
        .join("\n");
        let conversion = convert(&input).unwrap();
        assert_eq!(conversion.records.len(), 1);
        assert_eq!(conversion.records[0].source, "свод");
        assert_eq!(conversion.report.exclusions.len(), 3);
        assert!(
            conversion
                .report
                .exclusions
                .iter()
                .any(|item| item.reason == "unsupported_edit")
        );
    }

    #[test]
    fn legacy_project_cannot_claim_confirmed_asr_origin() {
        let mut value: serde_json::Value = serde_json::from_str(&item(
            5,
            "свод",
            Some("слот"),
            "reviewed",
            "real-asr",
            "slot",
        ))
        .unwrap();
        value["schema_version"] = serde_json::json!(1);
        let conversion = convert(&value.to_string()).unwrap();
        assert!(conversion.records.is_empty());
        assert_eq!(conversion.report.exclusions[0].reason, "origin_unconfirmed");
    }

    #[test]
    fn user_examples_with_word_merges_and_splits_validate() {
        let input = [
            item(
                11,
                "долгождать",
                Some("долго ждать"),
                "reviewed",
                "synthetic",
                "",
            ),
            item(
                12,
                "Нет звон",
                Some("недозвон"),
                "reviewed",
                "synthetic",
                "",
            ),
            item(
                13,
                "недозвода клиента",
                Some("недозвон до клиента"),
                "reviewed",
                "synthetic",
                "",
            ),
            item(
                14,
                "без снятия сваты",
                Some("без снятия слота"),
                "reviewed",
                "synthetic",
                "",
            ),
        ]
        .join("\n");
        let conversion = convert(&input).unwrap();
        assert_eq!(conversion.records.len(), 4);
        assert!(conversion.report.exclusions.is_empty());
        assert_eq!(conversion.records[0].edits[0].replacement, "долго ждать");
        assert_eq!(conversion.records[2].edits[0].replacement, "недозвон до");
    }
}
