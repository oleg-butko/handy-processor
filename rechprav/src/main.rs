use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use rechprav::{
    corpus::{self, Origin},
    metrics, review, text,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Parser)]
#[command(
    version,
    about = "Local ASR research workbench. Foundation version: KEEP only; no words are corrected."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum InputKind {
    Synthetic,
    RealAsr,
}

#[derive(Subcommand)]
enum Command {
    /// Create pending annotations from source-only UTF-8 lines, without guessing gold.
    Prepare {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        group: String,
        #[arg(long, value_enum)]
        kind: InputKind,
    },
    /// Validate JSONL schema, gold edits and leakage across all supplied splits.
    Validate {
        #[arg(required = true)]
        datasets: Vec<PathBuf>,
    },
    /// Write review.txt, corrected.txt and debug.jsonl to a NEW directory. KEEP only.
    Review {
        input: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 130)]
        width: usize,
        #[arg(long)]
        strict_yo: bool,
    },
    /// Evaluate the KEEP control baseline on one reviewed corpus/split.
    Benchmark {
        dataset: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        strict_yo: bool,
    },
    /// Inspect source byte ranges, lexical tokens and protected regions.
    Inspect {
        text: String,
        #[arg(long)]
        strict_yo: bool,
    },
}

fn new_file(path: &Path) -> Result<BufWriter<File>> {
    Ok(BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "create new file {}; existing files are never overwritten",
                    path.display()
                )
            })?,
    ))
}

fn new_dir(path: &Path) -> Result<()> {
    fs::create_dir(path).with_context(|| {
        format!(
            "create NEW output directory {}; parent must exist",
            path.display()
        )
    })
}

fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = new_file(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

fn manifest(input: &Path, contents: &[u8], command: &str) -> serde_json::Value {
    // Embed the source identity at compilation, independent of a later Git checkout.
    let sources: &[(&str, &[u8])] = &[
        ("Cargo.toml", include_bytes!("../Cargo.toml")),
        ("Cargo.lock", include_bytes!("../Cargo.lock")),
        ("src/lib.rs", include_bytes!("lib.rs")),
        ("src/main.rs", include_bytes!("main.rs")),
        ("src/corpus.rs", include_bytes!("corpus.rs")),
        ("src/metrics.rs", include_bytes!("metrics.rs")),
        ("src/review.rs", include_bytes!("review.rs")),
        ("src/text.rs", include_bytes!("text.rs")),
    ];
    let source_hashes: std::collections::BTreeMap<_, _> = sources
        .iter()
        .map(|(name, bytes)| (*name, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    json!({
        "schema_version":1, "command":command, "engine":"keep", "rechprav_version":env!("CARGO_PKG_VERSION"),
        "input_file":input.display().to_string(), "input_sha256":format!("{:x}",Sha256::digest(contents)),
        "arguments":std::env::args().skip(1).collect::<Vec<_>>(),
        "compiled_source_sha256":source_hashes,
        "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
        "unix_timestamp":SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()),
        "note":"KEEP infrastructure run; not an algorithm quality comparison"
    })
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Prepare {
            input,
            output,
            group,
            kind,
        } => {
            let contents = fs::read_to_string(&input).context("read input as UTF-8")?;
            let kind = match kind {
                InputKind::Synthetic => Origin::Synthetic,
                InputKind::RealAsr => Origin::RealAsr,
            };
            let records = corpus::prepare(&contents, &group, kind)?;
            let mut file = new_file(&output)?;
            for record in &records {
                serde_json::to_writer(&mut file, record)?;
                writeln!(file)?;
            }
            file.flush()?;
            println!(
                "{} pending annotations; gold=null. Output: {}",
                records.len(),
                output.display()
            );
        }
        Command::Validate { datasets } => {
            let mut records = Vec::new();
            for path in datasets {
                records.extend(corpus::read(&path)?);
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&corpus::validate(&records)?)?
            );
        }
        Command::Review {
            input,
            output_dir,
            width,
            strict_yo,
        } => {
            ensure!(
                (32..=130).contains(&width),
                "width must be between 32 and 130"
            );
            let contents = fs::read_to_string(&input).context("read input as UTF-8")?;
            new_dir(&output_dir)?;
            let mut human = new_file(&output_dir.join("review.txt"))?;
            let mut debug = new_file(&output_dir.join("debug.jsonl"))?;
            for line in text::lines(&contents) {
                let (rendered, record) = review::render(&line, width, strict_yo)?;
                human.write_all(rendered.as_bytes())?;
                serde_json::to_writer(&mut debug, &record)?;
                writeln!(debug)?;
            }
            human.flush()?;
            debug.flush()?;
            let mut corrected = new_file(&output_dir.join("corrected.txt"))?;
            corrected.write_all(contents.as_bytes())?;
            corrected.flush()?;
            save_json(
                &output_dir.join("manifest.json"),
                &manifest(&input, contents.as_bytes(), "review"),
            )?;
            println!("KEEP review: {} (no corrections)", output_dir.display());
        }
        Command::Benchmark {
            dataset,
            output_dir,
            strict_yo,
        } => {
            let contents = fs::read(&dataset)?;
            let records = corpus::read(&dataset)?;
            let started = Instant::now();
            let report = metrics::benchmark_keep(&records, strict_yo)?;
            let elapsed = started.elapsed();
            new_dir(&output_dir)?;
            save_json(&output_dir.join("metrics.json"), &report)?;
            let mut manifest = manifest(&dataset, &contents, "benchmark");
            manifest["evaluation_wall_ns"] = json!(elapsed.as_nanos());
            manifest["timing_scope"] =
                json!("validation+tokenization+metrics, NOT correction latency");
            save_json(&output_dir.join("manifest.json"), &manifest)?;
            let mut predictions = new_file(&output_dir.join("predictions.jsonl"))?;
            for record in records
                .iter()
                .filter(|r| r.annotation_status == corpus::Status::Reviewed)
            {
                serde_json::to_writer(
                    &mut predictions,
                    &json!({"id":record.id,"prediction":record.source,"engine":"keep"}),
                )?;
                writeln!(predictions)?;
            }
            predictions.flush()?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Inspect {
            text: source,
            strict_yo,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"source":source,"tokens":text::tokenize(&source, strict_yo)})
                )?
            );
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
