//! Synthetic warm analysis microbenchmark, separate from validation and JSON output.
use anyhow::{Result, ensure};
use clap::Parser;
use rechprav::{
    corpus,
    corrector::{Baseline, Corrector, analyze},
};
use serde::{Deserialize, Serialize};
use std::{hint::black_box, path::PathBuf, time::Instant};

#[derive(Parser)]
struct Args {
    config: PathBuf,
    corpus: PathBuf,
    #[arg(long, default_value_t = 100)]
    repetitions: usize,
    /// Optional normalized generator queries, independent of the annotated corpus.
    #[arg(long)]
    queries: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    lookup_repetitions: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Query {
    id: String,
    query: String,
    category: String,
    distance: usize,
    max_candidates: usize,
    max_nodes: usize,
}

fn stats(mut samples: Vec<u128>) -> serde_json::Value {
    samples.sort_unstable();
    let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100) - 1];
    serde_json::json!({"samples":samples.len(),"p50_ns":percentile(50),
        "p95_ns":percentile(95),"p99_ns":percentile(99),"max_ns":samples.last(),
        "mean_ns":samples.iter().sum::<u128>() as f64 / samples.len() as f64})
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
        (1..=10000).contains(&args.repetitions),
        "repetitions must be 1..10000"
    );
    ensure!(
        (1..=1000).contains(&args.lookup_repetitions),
        "lookup repetitions must be 1..1000"
    );
    let query_data = args.queries.as_ref().map(std::fs::read).transpose()?;
    let queries: Vec<Query> = query_data
        .as_ref()
        .map(|data| serde_json::from_slice(data))
        .transpose()?
        .unwrap_or_default();
    ensure!(queries.len() <= 10000, "too many benchmark queries");
    let mut ids = std::collections::BTreeSet::new();
    for query in &queries {
        ensure!(
            ids.insert(&query.id)
                && query.distance <= 2
                && query.query.chars().count() <= 256
                && query.max_candidates <= 1000
                && query.max_nodes <= 1000000,
            "invalid benchmark query/duplicate id"
        );
    }
    let records = corpus::read(&args.corpus)?;
    corpus::validate(&records)?;
    ensure!(!records.is_empty(), "empty corpus");
    let start = Instant::now();
    let (engine, resources, load_phases) = Baseline::load_profiled(&args.config)?;
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
    let mut lookup_checks = Vec::new();
    let mut lookup_samples = std::collections::BTreeMap::<String, Vec<u128>>::new();
    for query in &queries {
        let lookup = || {
            engine.dictionary.lookup(
                &query.query,
                query.distance,
                query.max_candidates,
                query.max_nodes,
            )
        };
        let search = lookup(); // Warmup and semantic snapshot; independent of protection/OOV gates.
        let complete = search.complete;
        lookup_checks.push(serde_json::json!({"request":query,"known":engine.dictionary.contains(&query.query),"search":search}));
        let group = format!(
            "distance_{}_{}",
            query.distance,
            if complete { "complete" } else { "limited" }
        );
        let length_group = match query.query.chars().count() {
            0..=4 => "length_0_4",
            5..=8 => "length_5_8",
            9..=16 => "length_9_16",
            17..=32 => "length_17_32",
            _ => "length_33_plus",
        };
        for _ in 0..args.lookup_repetitions {
            let start = Instant::now();
            let result = black_box(lookup());
            let ns = start.elapsed().as_nanos();
            drop(result);
            lookup_samples.entry(group.clone()).or_default().push(ns);
            lookup_samples
                .entry(length_group.to_owned())
                .or_default()
                .push(ns);
        }
    }
    let lookup_stats: std::collections::BTreeMap<_, _> = lookup_samples
        .into_iter()
        .map(|(k, v)| (k, stats(v)))
        .collect();
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
            "load_phases_ns":load_phases,
            "load_phases_scope":"dictionary.validation.* are nested inside dictionary.decode; all other phases are sequential; total includes timing/bookkeeping overhead, excludes final local-buffer destruction",
            "compiled_source_sha256":compiled_sources,
            "warm_record_p50_ns":percentile(50), "warm_record_p95_ns":percentile(95),
            "warm_record_p99_ns":percentile(99),
            "warm_record_max_ns":durations.last(), "samples":durations.len(),
            "records":records.len(), "repetitions":args.repetitions,
            "process_memory_linux":memory, "resources":resources,
            "queries_sha256":query_data.as_ref().map(|data| rechprav::hashing::sha256_hex(data)),
            "lookup_checks":lookup_checks,"lookup_statistics":lookup_stats,
            "lookup_scope":"normalized direct generator queries; includes DP allocation, traversal, candidates and sort, excludes result drop and serialization; bypasses protection/OOV/scoring; per-query warm repetitions",
            "scope":"warm analyze per synthetic record: tokenization, gates, lookup, scoring, reconstruction; excludes result drop, validation, serialization; load includes reading/hashing/building resources; OS page cache uncontrolled; not production latency"
        }))?
    );
    Ok(())
}
