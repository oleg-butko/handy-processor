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
pub struct BenchmarkReport {
    pub schema_version: u32,
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
    pub proposals_in_unsupported_spans: usize,
    pub proposed_edits: usize,
    pub corrected_gold_spans: usize,
    pub gold_spans: usize,
    pub overall_precision: Option<f64>,
    pub all_gold_span_recall: Option<f64>,
    pub wer_errors_before: WordErrors,
    pub wer_errors_after: WordErrors,
    pub wer_before: Option<f64>,
    pub wer_after: Option<f64>,
    pub coverage: Option<f64>,
    pub candidate_error_positions: usize,
    pub candidate_hits_at_k: std::collections::BTreeMap<usize, usize>,
    pub candidate_recall_at_k: std::collections::BTreeMap<usize, Option<f64>>,
    pub candidate_recall_scope: &'static str,
    pub incomplete_searches: usize,
    pub quality_claim: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Prediction {
    pub id: String,
    pub source: String,
    pub gold: String,
    pub prediction: String,
    pub tags: Vec<String>,
    pub edits: Vec<crate::corpus::Edit>,
    pub decisions: Vec<crate::corrector::Decision>,
}

pub struct BenchmarkRun {
    pub report: BenchmarkReport,
    pub predictions: Vec<Prediction>,
}

pub fn benchmark_keep(records: &[Record], strict_yo: bool) -> Result<BenchmarkReport> {
    Ok(benchmark(records, &crate::corrector::Keep { strict_yo })?.report)
}

pub fn benchmark(
    records: &[Record],
    engine: &dyn crate::corrector::Corrector,
) -> Result<BenchmarkRun> {
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
    let strict_yo = engine.strict_yo();
    let mut counts = Counts::default();
    let mut before = WordErrors::default();
    let mut after = WordErrors::default();
    let (mut source_tokens, mut gold_tokens, mut unsupported) = (0, 0, 0);
    let (mut proposals, mut correct, mut total_gold, mut unsupported_proposals) = (0, 0, 0, 0);
    let (mut candidate_errors, mut incomplete) = (0, 0);
    let mut hits: std::collections::BTreeMap<usize, usize> =
        [1, 3, 5, 10, 20].into_iter().map(|k| (k, 0)).collect();
    let mut predictions = Vec::new();
    for record in reviewed {
        let analysis = crate::corrector::analyze(&record.source, engine);
        let source = &analysis.tokens;
        let gold = tokenize(record.gold.as_ref().unwrap(), strict_yo);
        let predicted = tokenize(&analysis.corrected, strict_yo);
        ensure!(
            source.lexical.len() == predicted.lexical.len(),
            "evaluator currently requires 1-to-1 predictions"
        );
        source_tokens += source.lexical.len();
        gold_tokens += gold.lexical.len();
        proposals += analysis.edits.len();
        total_gold += record.edits.len();
        incomplete += analysis
            .decisions
            .iter()
            .filter(|d| !d.search_complete)
            .count();
        let mut expected: Vec<Option<&str>> = source
            .lexical
            .iter()
            .map(|t| Some(&record.source[t.byte_start..t.byte_end]))
            .collect();
        for edit in &record.edits {
            let replacement = tokenize(&edit.replacement, strict_yo);
            if edit.token_end == edit.token_start + 1 && replacement.lexical.len() == 1 {
                expected[edit.token_start] = Some(&edit.replacement);
                candidate_errors += 1;
                let candidates = &analysis.decisions[edit.token_start].alternatives;
                for (&k, count) in &mut hits {
                    if candidates
                        .iter()
                        .take(k)
                        .any(|c| c.key == replacement.lexical[0].key)
                    {
                        *count += 1;
                    }
                }
            } else {
                unsupported += 1;
                expected[edit.token_start..edit.token_end].fill(None);
            }
            if analysis.edits.iter().any(|p| {
                p.byte_start == edit.byte_start
                    && p.byte_end == edit.byte_end
                    && p.replacement == edit.replacement
            }) {
                correct += 1;
            }
        }
        for (i, decision) in analysis.decisions.iter().enumerate() {
            if let Some(gold) = expected[i] {
                counts.observe(&decision.original, gold, &decision.candidate);
            } else if decision.changed() {
                unsupported_proposals += 1;
            }
        }
        let reference: Vec<_> = gold.lexical.into_iter().map(|t| t.key).collect();
        before.add(word_errors(
            &reference,
            &source
                .lexical
                .iter()
                .map(|t| t.key.clone())
                .collect::<Vec<_>>(),
        ));
        after.add(word_errors(
            &reference,
            &predicted
                .lexical
                .into_iter()
                .map(|t| t.key)
                .collect::<Vec<_>>(),
        ));
        predictions.push(Prediction {
            id: record.id.clone(),
            source: record.source.clone(),
            gold: record.gold.clone().unwrap(),
            prediction: analysis.corrected,
            tags: record.tags.clone(),
            edits: analysis.edits,
            decisions: analysis.decisions,
        });
    }
    Ok(BenchmarkRun {
        report: BenchmarkReport {
            schema_version: 2,
            engine: engine.name(),
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
            proposals_in_unsupported_spans: unsupported_proposals,
            proposed_edits: proposals,
            corrected_gold_spans: correct,
            gold_spans: total_gold,
            overall_precision: ratio(correct, proposals),
            all_gold_span_recall: ratio(correct, total_gold),
            wer_errors_before: before,
            wer_errors_after: after,
            wer_before: ratio(before.total(), gold_tokens),
            wer_after: ratio(after.total(), gold_tokens),
            coverage: ratio(proposals, source_tokens),
            candidate_error_positions: candidate_errors,
            candidate_recall_at_k: hits
                .iter()
                .map(|(&k, &n)| (k, ratio(n, candidate_errors)))
                .collect(),
            candidate_hits_at_k: hits,
            candidate_recall_scope: "emitted candidates after protection/OOV gates, denominator = all annotated 1-to-1 errors; not ungated generator recall",
            incomplete_searches: incomplete,
            quality_claim: if origin == Origin::Synthetic {
                "technical fixtures only; not quality on real ASR"
            } else {
                "experimental result; no automatic production approval"
            },
        },
        predictions,
    })
}
