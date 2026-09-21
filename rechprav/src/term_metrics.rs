//! Exact source-span scoring supports lexical term and multiword phrase outputs.
use crate::{
    corpus::{self, Record, Status},
    metrics::{Counts, WordErrors, ratio, word_errors},
    terms::TermEngine,
    text::tokenize,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub struct Run {
    pub report: Value,
    pub predictions: Vec<Value>,
}

pub fn benchmark(records: &[Record], engine: &TermEngine) -> Result<Run> {
    let validation = corpus::validate(records)?;
    ensure!(
        validation.pending == 0 && validation.reviewed > 0,
        "require reviewed corpus without pending records"
    );
    let reviewed: Vec<_> = records
        .iter()
        .filter(|r| r.annotation_status == Status::Reviewed)
        .collect();
    let first = reviewed[0];
    ensure!(
        reviewed
            .iter()
            .all(|r| r.split == first.split && r.provenance.kind == first.provenance.kind),
        "benchmark one split and origin at a time"
    );
    let mut counts = Counts::default();
    let mut before = WordErrors::default();
    let mut after = WordErrors::default();
    let (mut gold_tokens, mut source_tokens, mut exact, mut incomplete) = (0, 0, 0, 0);
    let mut predictions = Vec::new();
    // Same normalization as corpus validation; exact span correctness remains case-sensitive.
    for record in reviewed {
        let analysis = engine.analyze(&record.source);
        let source = tokenize(&record.source, false);
        let gold = tokenize(record.gold.as_ref().unwrap(), false);
        let predicted = tokenize(&analysis.corrected, false);
        let tp = analysis
            .edits
            .iter()
            .filter(|p| {
                record.edits.iter().any(|g| {
                    p.byte_start == g.byte_start
                        && p.byte_end == g.byte_end
                        && p.replacement == g.replacement
                })
            })
            .count();
        counts.tp += tp;
        counts.fp += analysis.edits.len() - tp;
        counts.fn_ += record.edits.len() - tp;
        for position in 0..source.lexical.len() {
            if !record
                .edits
                .iter()
                .any(|e| (e.token_start..e.token_end).contains(&position))
            {
                counts.correct_input += 1;
                if analysis
                    .edits
                    .iter()
                    .any(|e| (e.token_start..e.token_end).contains(&position))
                {
                    counts.harmed_correct_input += 1;
                }
            }
        }
        let reference: Vec<_> = gold.lexical.iter().map(|t| t.key.clone()).collect();
        for (total, hypothesis) in [(&mut before, &source), (&mut after, &predicted)] {
            let error = word_errors(
                &reference,
                &hypothesis
                    .lexical
                    .iter()
                    .map(|t| t.key.clone())
                    .collect::<Vec<_>>(),
            );
            total.substitutions += error.substitutions;
            total.deletions += error.deletions;
            total.insertions += error.insertions;
        }
        source_tokens += source.lexical.len();
        gold_tokens += gold.lexical.len();
        exact += usize::from(Some(&analysis.corrected) == record.gold.as_ref());
        incomplete += usize::from(!analysis.complete);
        predictions.push(json!({"id":record.id,"source":record.source,"gold":record.gold,"tags":record.tags,
            "prediction":analysis.corrected,"edits":analysis.edits,"decisions":analysis.decisions,"complete":analysis.complete,"comparisons":analysis.comparisons}));
    }
    Ok(Run {
        report: json!({"schema_version":1,"engine":"term_spans","dataset_kind":first.provenance.kind,"split":first.split,
        "reviewed_records":validation.reviewed,"excluded_ambiguous_records":validation.ambiguous,
        "source_tokens":source_tokens,"gold_tokens":gold_tokens,"span_counts":counts,"rates":counts.rates(),
        "exact_records":exact,"incomplete_records":incomplete,"wer_errors_before":before,"wer_errors_after":after,
        "wer_before":ratio(before.total(),gold_tokens),"wer_after":ratio(after.total(),gold_tokens),
        "scope":"TP/FP/FN require exact source byte span and case-sensitive replacement; harms count touched correct input tokens once; WER uses case-insensitive yo-folded lexical tokens; no token zip",
        "quality_claim": if first.provenance.kind == corpus::Origin::RealAsr {
            "experimental result on the supplied ASR corpus; check provenance and split; development or learned examples are not independent quality estimates; scores are not probabilities"
        } else {
            "experimental corpus result; synthetic fixtures do not estimate real ASR quality; scores are not probabilities"
        }}),
        predictions,
    })
}
