//! Deterministic group-level pilot split. No row-wise random split is allowed.
use crate::{corpus, hashing};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Serialize)]
pub struct SplitReport {
    pub seed: String,
    pub records: usize,
    pub groups: usize,
    pub train_records: usize,
    pub dev_records: usize,
    pub test_records: usize,
    pub train_groups: usize,
    pub dev_groups: usize,
    pub test_groups: usize,
    pub provenance_kind: &'static str,
}

pub struct SplitOutput {
    pub train: Vec<corpus::Record>,
    pub dev: Vec<corpus::Record>,
    pub test: Vec<corpus::Record>,
    pub report: SplitReport,
}

pub fn split(mut records: Vec<corpus::Record>, seed: &str) -> Result<SplitOutput> {
    ensure!(
        !seed.trim().is_empty() && seed.len() <= 120,
        "seed must have 1..120 bytes"
    );
    corpus::validate(&records)?;
    ensure!(
        records
            .iter()
            .all(|record| record.split == corpus::Split::Unassigned),
        "input already contains assigned splits"
    );
    ensure!(
        records
            .iter()
            .all(|record| record.annotation_status == corpus::Status::Reviewed),
        "split only a reviewed corpus"
    );
    let origin = records[0].provenance.kind;
    ensure!(
        records
            .iter()
            .all(|record| record.provenance.kind == origin),
        "split real ASR and synthetic corpora separately"
    );
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    let mut phrase_group = HashMap::<String, String>::new();
    for (index, record) in records.iter().enumerate() {
        groups
            .entry(record.group_id.clone())
            .or_default()
            .push(index);
        for phrase in std::iter::once(record.source.as_str()).chain(record.gold.as_deref()) {
            let key = corpus::leakage_key(phrase);
            if let Some(previous) = phrase_group.insert(key, record.group_id.clone()) {
                ensure!(
                    previous == record.group_id,
                    "normalized source/gold appears in different groups; merge groups before splitting"
                );
            }
        }
    }
    let mut ordered = groups
        .into_iter()
        .map(|(name, members)| {
            let hash = hashing::sha256_hex(format!("{seed}\0{name}").as_bytes());
            (hash, members)
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|(hash_a, rows_a), (hash_b, rows_b)| {
        rows_b
            .len()
            .cmp(&rows_a.len())
            .then_with(|| hash_a.cmp(hash_b))
    });
    let total = records.len();
    let targets = [total as f64 * 0.6, total as f64 * 0.2, total as f64 * 0.2];
    let mut counts = [0usize; 3];
    let mut group_counts = [0usize; 3];
    for (_, members) in &ordered {
        let chosen = (0..3)
            .min_by(|&a, &b| {
                let score = |candidate| {
                    (0..3)
                        .map(|part| {
                            let after =
                                counts[part] + usize::from(part == candidate) * members.len();
                            let delta = after as f64 - targets[part];
                            delta * delta / targets[part].max(1.0)
                        })
                        .sum::<f64>()
                };
                score(a).total_cmp(&score(b)).then(a.cmp(&b))
            })
            .unwrap();
        counts[chosen] += members.len();
        group_counts[chosen] += 1;
        let split = [
            corpus::Split::Train,
            corpus::Split::Dev,
            corpus::Split::Test,
        ][chosen];
        for &index in members {
            records[index].split = split;
        }
    }
    corpus::validate(&records)?;
    let report = SplitReport {
        seed: seed.to_string(),
        records: total,
        groups: ordered.len(),
        train_records: counts[0],
        dev_records: counts[1],
        test_records: counts[2],
        train_groups: group_counts[0],
        dev_groups: group_counts[1],
        test_groups: group_counts[2],
        provenance_kind: match origin {
            corpus::Origin::RealAsr => "real-asr",
            corpus::Origin::Synthetic => "synthetic",
        },
    };
    let mut train = Vec::new();
    let mut dev = Vec::new();
    let mut test = Vec::new();
    for record in records {
        match record.split {
            corpus::Split::Train => train.push(record),
            corpus::Split::Dev => dev.push(record),
            corpus::Split::Test => test.push(record),
            corpus::Split::Unassigned => unreachable!(),
        }
    }
    Ok(SplitOutput {
        train,
        dev,
        test,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::split;
    use crate::corpus::{self, Origin, Provenance, Record, Split, Status};
    use std::collections::BTreeMap;

    fn row(id: usize, group: &str, source: &str) -> Record {
        Record {
            schema_version: 1,
            id: format!("r{id}"),
            group_id: group.into(),
            split: Split::Unassigned,
            source: source.into(),
            gold: Some(source.into()),
            edits: vec![],
            tags: vec![],
            provenance: Provenance {
                kind: Origin::Synthetic,
                origin: format!("fixture:{id}"),
                asr_model: None,
            },
            annotation_status: Status::Reviewed,
        }
    }

    fn assignments(rows: &[Record]) -> BTreeMap<String, Split> {
        rows.iter().map(|row| (row.id.clone(), row.split)).collect()
    }

    #[test]
    fn same_seed_is_independent_of_input_order_and_keeps_groups_together() {
        let rows = (0..30)
            .map(|i| row(i, &format!("g{i}"), &format!("слово{i}")))
            .collect::<Vec<_>>();
        let first = split(rows.clone(), "pilot-1").unwrap();
        let second = split(rows.into_iter().rev().collect(), "pilot-1").unwrap();
        let combine = |result: super::SplitOutput| [result.train, result.dev, result.test].concat();
        let a = combine(first);
        let b = combine(second);
        assert_eq!(assignments(&a), assignments(&b));
        assert!(a.iter().any(|row| row.split == Split::Train));
        assert!(a.iter().any(|row| row.split == Split::Dev));
        assert!(a.iter().any(|row| row.split == Split::Test));
        corpus::validate(&a).unwrap();
    }

    #[test]
    fn overlapping_phrases_in_different_groups_are_rejected_before_split() {
        let input = vec![row(1, "a", "свод"), row(2, "b", "свод")];
        let error = split(input, "pilot-1").err().unwrap().to_string();
        assert!(error.contains("different groups"), "{error}");
    }
}
