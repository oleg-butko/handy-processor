use crate::{
    corpus::{Origin, Record, Split, Status, validate},
    text::tokenize,
};
use anyhow::{Result, ensure};
use serde::Serialize;

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
    pub correct_input: usize,
    pub harmed_correct_input: usize,
}

pub fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator != 0).then(|| numerator as f64 / denominator as f64)
}

#[derive(Debug, Serialize)]
pub struct Rates {
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f05: Option<f64>,
    pub f1: Option<f64>,
    pub false_correction_rate: Option<f64>,
}

impl Counts {
    pub fn rates(&self) -> Rates {
        Rates {
            precision: ratio(self.tp, self.tp + self.fp),
            recall: ratio(self.tp, self.tp + self.fn_),
            f05: ratio(5 * self.tp, 5 * self.tp + self.fn_ + 4 * self.fp),
            f1: ratio(2 * self.tp, 2 * self.tp + self.fn_ + self.fp),
            false_correction_rate: ratio(self.harmed_correct_input, self.correct_input),
        }
    }

    /// Use only for explicitly aligned 1→1 positions, never zip unaligned sentences.
    pub fn observe(&mut self, source: &str, gold: &str, prediction: &str) {
        if source == gold {
            self.correct_input += 1;
            if prediction != gold {
                self.harmed_correct_input += 1;
            }
        } else if prediction == gold {
            self.tp += 1;
        } else {
            self.fn_ += 1;
        }
        if prediction != source && prediction != gold {
            self.fp += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct WordErrors {
    pub substitutions: usize,
    pub deletions: usize,
    pub insertions: usize,
}

impl WordErrors {
    pub fn total(self) -> usize {
        self.substitutions + self.deletions + self.insertions
    }
    fn add(&mut self, rhs: Self) {
        self.substitutions += rhs.substitutions;
        self.deletions += rhs.deletions;
        self.insertions += rhs.insertions;
    }
}

/// O(reference tokens) memory. Ties prefer diagonal, then deletion, then insertion.
pub fn word_errors(reference: &[String], hypothesis: &[String]) -> WordErrors {
    let mut row: Vec<WordErrors> = (0..=reference.len())
        .map(|n| WordErrors {
            deletions: n,
            ..Default::default()
        })
        .collect();
    for (i, word) in hypothesis.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = WordErrors {
            insertions: i + 1,
            ..Default::default()
        };
        for (j, gold) in reference.iter().enumerate() {
            let old = row[j + 1];
            let mut subst = diagonal;
            subst.substitutions += usize::from(word != gold);
            let mut deletion = row[j];
            deletion.deletions += 1;
            let mut insertion = old;
            insertion.insertions += 1;
            row[j + 1] = [subst, deletion, insertion]
                .into_iter()
                .min_by_key(|e| e.total())
                .unwrap();
            diagonal = old;
        }
    }
    row[reference.len()]
}

#[derive(Debug, Serialize)]
pub struct KeepReport {
    pub engine: &'static str,
    pub dataset_kind: Origin,
    pub split: Split,
    pub strict_yo: bool,
    pub reviewed_records: usize,
    pub excluded_ambiguous_records: usize,
    pub source_tokens: usize,
    pub gold_tokens: usize,
    pub counts_1_to_1: Counts,
    pub rates_1_to_1: Rates,
    pub unsupported_gold_spans: usize,
    pub all_error_recall: Option<f64>,
    pub wer_errors_before: WordErrors,
    pub wer_errors_after: WordErrors,
    pub wer_before: Option<f64>,
    pub wer_after: Option<f64>,
    pub coverage: Option<f64>,
    pub quality_claim: &'static str,
}

pub fn benchmark_keep(records: &[Record], strict_yo: bool) -> Result<KeepReport> {
    let summary = validate(records)?;
    ensure!(
        summary.pending == 0,
        "pending annotations cannot be benchmarked"
    );
    ensure!(summary.reviewed > 0, "no reviewed records to benchmark");
    let reviewed: Vec<_> = records
        .iter()
        .filter(|r| r.annotation_status == Status::Reviewed)
        .collect();
    let origin = reviewed[0].provenance.kind;
    let split = reviewed[0].split;
    ensure!(
        reviewed.iter().all(|r| r.provenance.kind == origin),
        "benchmark synthetic and real ASR separately"
    );
    ensure!(
        reviewed.iter().all(|r| r.split == split),
        "benchmark one split at a time"
    );
    let mut counts = Counts::default();
    let mut errors = WordErrors::default();
    let (mut source_tokens, mut gold_tokens, mut unsupported) = (0, 0, 0);
    for record in reviewed {
        let source = tokenize(&record.source, strict_yo);
        let gold = tokenize(record.gold.as_ref().unwrap(), strict_yo);
        source_tokens += source.lexical.len();
        gold_tokens += gold.lexical.len();
        let mut covered = vec![false; source.lexical.len()];
        for edit in &record.edits {
            covered[edit.token_start..edit.token_end].fill(true);
            let replacement = tokenize(&edit.replacement, strict_yo);
            if edit.token_end == edit.token_start + 1 && replacement.lexical.len() == 1 {
                let original = &record.source[edit.byte_start..edit.byte_end];
                counts.observe(original, &edit.replacement, original);
            } else {
                unsupported += 1;
            }
        }
        counts.correct_input += covered.iter().filter(|&&x| !x).count();
        errors.add(word_errors(
            &gold.lexical.into_iter().map(|t| t.key).collect::<Vec<_>>(),
            &source
                .lexical
                .into_iter()
                .map(|t| t.key)
                .collect::<Vec<_>>(),
        ));
    }
    Ok(KeepReport {
        engine: "keep",
        dataset_kind: origin,
        split,
        strict_yo,
        reviewed_records: summary.reviewed,
        excluded_ambiguous_records: summary.ambiguous,
        source_tokens,
        gold_tokens,
        rates_1_to_1: counts.rates(),
        counts_1_to_1: counts,
        unsupported_gold_spans: unsupported,
        all_error_recall: ratio(0, errors.total()),
        wer_errors_before: errors,
        wer_errors_after: errors,
        wer_before: ratio(errors.total(), gold_tokens),
        wer_after: ratio(errors.total(), gold_tokens),
        coverage: ratio(0, source_tokens),
        quality_claim: "control baseline only; no correction algorithm evaluated",
    })
}
