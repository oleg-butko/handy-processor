//! Warm term analysis timings; separate loading, verification and serialization.
use anyhow::{Result, ensure};
use clap::Parser;
use rechprav::{corpus, terms::TermEngine};
use std::{hint::black_box, path::PathBuf, time::Instant};
#[derive(Parser)]
struct Args {
    config: PathBuf,
    corpus: PathBuf,
    #[arg(long, default_value_t = 30)]
    repetitions: usize,
}
fn main() -> Result<()> {
    let compiled_sources: std::collections::BTreeMap<_, _> = [
        (
            "examples/term_profile.rs",
            include_bytes!("term_profile.rs").as_slice(),
        ),
        ("Cargo.toml", include_bytes!("../Cargo.toml").as_slice()),
        ("Cargo.lock", include_bytes!("../Cargo.lock").as_slice()),
        ("src/lib.rs", include_bytes!("../src/lib.rs").as_slice()),
        (
            "src/hashing.rs",
            include_bytes!("../src/hashing.rs").as_slice(),
        ),
        (
            "src/corpus.rs",
            include_bytes!("../src/corpus.rs").as_slice(),
        ),
        (
            "src/corrector.rs",
            include_bytes!("../src/corrector.rs").as_slice(),
        ),
        (
            "src/dictionary.rs",
            include_bytes!("../src/dictionary.rs").as_slice(),
        ),
        (
            "src/frequency.rs",
            include_bytes!("../src/frequency.rs").as_slice(),
        ),
        (
            "src/metrics.rs",
            include_bytes!("../src/metrics.rs").as_slice(),
        ),
        (
            "src/review.rs",
            include_bytes!("../src/review.rs").as_slice(),
        ),
        ("src/text.rs", include_bytes!("../src/text.rs").as_slice()),
        ("src/terms.rs", include_bytes!("../src/terms.rs").as_slice()),
        (
            "src/pronunciation.rs",
            include_bytes!("../src/pronunciation.rs").as_slice(),
        ),
        (
            "src/term_context.rs",
            include_bytes!("../src/term_context.rs").as_slice(),
        ),
        (
            "src/term_metrics.rs",
            include_bytes!("../src/term_metrics.rs").as_slice(),
        ),
        (
            "src/packed_frequency.rs",
            include_bytes!("../src/packed_frequency.rs").as_slice(),
        ),
        (
            "src/packed.rs",
            include_bytes!("../src/packed.rs").as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, rechprav::hashing::sha256_hex(bytes)))
    .collect();

    let args = Args::parse();
    ensure!(
        (1..=1000).contains(&args.repetitions),
        "invalid repetitions"
    );
    let records = corpus::read(&args.corpus)?;
    corpus::validate(&records)?;
    let start = Instant::now();
    let (engine, resources) = TermEngine::load(&args.config, false)?;
    let load_ns = start.elapsed().as_nanos();
    let mut comparisons = 0;
    let mut candidates = 0;
    let mut eligible = 0;
    for r in &records {
        let a = engine.analyze(&r.source);
        ensure!(a.complete, "incomplete warmup");
        comparisons += a.comparisons;
        for d in &a.decisions {
            candidates += d.candidates.len();
            eligible += d.candidates.iter().filter(|c| c.eligible).count();
        }
        black_box(a);
    }
    let mut samples = Vec::new();
    for _ in 0..args.repetitions {
        for r in &records {
            let start = Instant::now();
            let a = black_box(engine.analyze(black_box(&r.source)));
            samples.push(start.elapsed().as_nanos());
            drop(a);
        }
    }
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    let percentile = |p: usize| sorted[(sorted.len() * p).div_ceil(100) - 1];
    let report = serde_json::json!({"compiled_source_sha256":compiled_sources,"resources":resources,
        "corpus_sha256":rechprav::hashing::sha256_hex(&std::fs::read(&args.corpus)?),
        "resource_load_ns":load_ns,"repetitions":args.repetitions,"records":records.len(),
        "comparisons_one_pass":comparisons,"unique_candidates_before_gates_one_pass":candidates,"eligible_candidates_one_pass":eligible,
        "samples_ns":samples,"p50_ns":percentile(50),"p95_ns":percentile(95),"p99_ns":percentile(99),
        "scope":"warm per-record analyze incl tokenization/scoring/reconstruction; excludes load, result drop and JSON; uncontrolled OS cache, no production latency claim"});
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
