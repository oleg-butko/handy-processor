//! Synthetic warm analysis microbenchmark, separate from validation and JSON output.
use anyhow::{Result, ensure};
use clap::Parser;
use rechprav::{
    corpus,
    corrector::{Baseline, Corrector, analyze},
};
use sha2::{Digest, Sha256};
use std::{hint::black_box, path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    config: PathBuf,
    corpus: PathBuf,
    #[arg(long, default_value_t = 100)]
    repetitions: usize,
}

fn main() -> Result<()> {
    let compiled_sources: std::collections::BTreeMap<_, _> = [
        (
            "examples/profile.rs",
            include_bytes!("profile.rs").as_slice(),
        ),
        ("Cargo.toml", include_bytes!("../Cargo.toml").as_slice()),
        ("Cargo.lock", include_bytes!("../Cargo.lock").as_slice()),
        ("src/lib.rs", include_bytes!("../src/lib.rs").as_slice()),
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
    ]
    .into_iter()
    .map(|(name, bytes)| (name, format!("{:x}", Sha256::digest(bytes))))
    .collect();
    let args = Args::parse();
    ensure!(
        (1..=10000).contains(&args.repetitions),
        "repetitions must be 1..10000"
    );
    let records = corpus::read(&args.corpus)?;
    corpus::validate(&records)?;
    ensure!(!records.is_empty(), "empty corpus");
    let start = Instant::now();
    let (engine, resources) = Baseline::load(&args.config)?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    for record in &records {
        black_box(analyze(&record.source, &engine));
    }
    let mut durations = Vec::new();
    for _ in 0..args.repetitions {
        for record in &records {
            let start = Instant::now();
            let result = black_box(analyze(black_box(&record.source), &engine));
            durations.push(start.elapsed().as_nanos());
            drop(result);
        }
    }
    durations.sort_unstable();
    let percentile = |p: usize| durations[(durations.len() * p).div_ceil(100) - 1];
    let memory = std::fs::read_to_string("/proc/self/status")
        .ok()
        .map(|status| {
            status
                .lines()
                .filter(|l| l.starts_with("VmRSS:") || l.starts_with("VmHWM:"))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "engine": engine.name(), "resource_load_ms": load_ms,
            "compiled_source_sha256":compiled_sources,
            "warm_record_p50_ns":percentile(50), "warm_record_p95_ns":percentile(95),
            "warm_record_max_ns":durations.last(), "samples":durations.len(),
            "records":records.len(), "repetitions":args.repetitions,
            "process_memory_linux":memory, "resources":resources,
            "scope":"warm analyze per synthetic record: tokenization, gates, lookup, scoring, reconstruction; excludes result drop, validation, serialization; load includes reading/hashing/building resources; OS page cache uncontrolled; not production latency"
        }))?
    );
    Ok(())
}
