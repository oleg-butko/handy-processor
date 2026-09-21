//! Create disabled, reviewable term suggestions from a local event journal.
#[path = "../src/feedback.rs"]
mod feedback;
use anyhow::{Context, Result, ensure};
use clap::Parser;
use rechprav::{corrector::Baseline, hashing::sha256_hex, terms::Config};
use std::{fs, io::Write, path::PathBuf};
#[derive(Parser)]
struct Args {
    events: PathBuf,
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    scope: String,
    #[arg(long)]
    output_dir: PathBuf,
}
fn main() -> Result<()> {
    let compiled_sources: std::collections::BTreeMap<_, _> = [
        (
            "examples/feedback_draft.rs",
            include_bytes!("feedback_draft.rs").as_slice(),
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
        (
            "src/feedback.rs",
            include_bytes!("../src/feedback.rs").as_slice(),
        ),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, rechprav::hashing::sha256_hex(bytes)))
    .collect();

    let args = Args::parse();
    ensure!(
        fs::metadata(&args.events)?.len() <= 64 * 1024 * 1024,
        "journal exceeds 64 MiB"
    );
    let contents = fs::read_to_string(&args.events)?;
    let events: Vec<feedback::Event> = contents
        .lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line).with_context(|| format!("parse event line {}", i + 1))
        })
        .collect::<Result<_>>()?;
    let config_bytes = fs::read_to_string(&args.config)?;
    let mut config: Config = toml::from_str(&config_bytes)?;
    config.validate()?;
    let path = args
        .config
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(&config.baseline_config)
        .canonicalize()?;
    config.baseline_config = path.clone();
    let (base, resources) = Baseline::load(&path)?;
    let report = feedback::draft(&events, &args.scope, &config, &base)?;
    let report_json = serde_json::to_string_pretty(&report)? + "\n";
    let draft_toml = toml::to_string_pretty(&report.config)?;
    let manifest = serde_json::json!({"compiled_source_sha256":compiled_sources,"resources":resources,
        "input_file":args.events.canonicalize()?,"input_sha256":sha256_hex(contents.as_bytes()),
        "reference_config_sha256":sha256_hex(config_bytes.as_bytes()),"scope":args.scope,
        "output_sha256":{"report.json":sha256_hex(report_json.as_bytes()),"proposals.toml":sha256_hex(draft_toml.as_bytes())},
        "quality_claim":"disabled offline proposals; no automatic learning, acceptance, dictionary edits or editor integration"});
    fs::create_dir(&args.output_dir)
        .context("output directory must be new; journal validation completed before creation")?;
    for (name, data) in [
        ("report.json", report_json),
        ("proposals.toml", draft_toml),
        (
            "draft-manifest.json",
            serde_json::to_string_pretty(&manifest)? + "\n",
        ),
    ] {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(args.output_dir.join(name))?;
        file.write_all(data.as_bytes())?;
        file.flush()?;
    }
    println!(
        "{} disabled proposals in {}",
        report.proposals.len(),
        args.output_dir.display()
    );
    Ok(())
}
