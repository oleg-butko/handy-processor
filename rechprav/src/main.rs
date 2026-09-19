use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rechprav::{
    corpus::{self, Origin},
    corrector::{Baseline, Corrector, Keep, analyze},
    dictionary::Dictionary,
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
    about = "Local ASR workbench: KEEP control or experimental dictionary correction with --config."
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

#[derive(Clone, Copy, ValueEnum)]
enum Engine {
    Keep,
    Baseline,
}

#[derive(Args)]
struct EngineArgs {
    /// Defaults to baseline with --config, otherwise KEEP.
    #[arg(long, value_enum)]
    engine: Option<Engine>,
    #[arg(long)]
    config: Option<PathBuf>,
    /// For KEEP only; baseline takes normalization from TOML.
    #[arg(long)]
    strict_yo: bool,
}

impl EngineArgs {
    fn load(self) -> Result<(Box<dyn Corrector>, serde_json::Value)> {
        let choice = self.engine.unwrap_or(if self.config.is_some() {
            Engine::Baseline
        } else {
            Engine::Keep
        });
        match choice {
            Engine::Keep => {
                ensure!(self.config.is_none(), "KEEP does not take --config");
                Ok((
                    Box::new(Keep {
                        strict_yo: self.strict_yo,
                    }),
                    json!({"strict_yo":self.strict_yo}),
                ))
            }
            Engine::Baseline => {
                ensure!(
                    !self.strict_yo,
                    "set strict_yo in baseline TOML, not on the command line"
                );
                let config = self.config.context("baseline requires --config")?;
                let (engine, metadata) = Baseline::load(&config)?;
                Ok((Box::new(engine), metadata))
            }
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Normalize and validate a word list. Trie is built once when the baseline starts.
    BuildDict {
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        strict_yo: bool,
    },
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
    /// Write review, corrected text and debug to a NEW directory. Use --config to correct.
    #[command(visible_alias = "correct")]
    Review {
        input: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 130)]
        width: usize,
        #[command(flatten)]
        engine: EngineArgs,
    },
    /// Evaluate one engine on one reviewed corpus/split.
    Benchmark {
        dataset: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[command(flatten)]
        engine: EngineArgs,
    },
    /// Inspect source ranges, protection, candidates and decisions.
    Inspect {
        text: String,
        #[command(flatten)]
        engine: EngineArgs,
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
        ("src/corrector.rs", include_bytes!("corrector.rs")),
        ("src/dictionary.rs", include_bytes!("dictionary.rs")),
        ("src/metrics.rs", include_bytes!("metrics.rs")),
        ("src/review.rs", include_bytes!("review.rs")),
        ("src/text.rs", include_bytes!("text.rs")),
    ];
    let source_hashes: std::collections::BTreeMap<_, _> = sources
        .iter()
        .map(|(name, bytes)| (*name, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    json!({
        "schema_version":2, "command":command, "rechprav_version":env!("CARGO_PKG_VERSION"),
        "input_file":input.display().to_string(), "input_sha256":format!("{:x}",Sha256::digest(contents)),
        "arguments":std::env::args().skip(1).collect::<Vec<_>>(),
        "compiled_source_sha256":source_hashes,
        "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
        "unix_timestamp":SystemTime::now().duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs()),
        "note":"experimental run; check dataset provenance before interpreting quality"
    })
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::BuildDict {
            input,
            output,
            strict_yo,
        } => {
            let contents = fs::read_to_string(input)?;
            let dictionary = Dictionary::from_text(&contents, strict_yo)?;
            save_json(&output, &dictionary.export())?;
            println!(
                "{} normalized words; {} runtime trie nodes",
                dictionary.len(),
                dictionary.node_count()
            );
        }
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
            engine,
        } => {
            ensure!(
                (32..=130).contains(&width),
                "width must be between 32 and 130"
            );
            let contents = fs::read_to_string(&input).context("read input as UTF-8")?;
            let (engine, metadata) = engine.load()?;
            new_dir(&output_dir)?;
            let mut human = new_file(&output_dir.join("review.txt"))?;
            let mut debug = new_file(&output_dir.join("debug.jsonl"))?;
            let mut corrected = new_file(&output_dir.join("corrected.txt"))?;
            for line in text::lines(&contents) {
                let (rendered, record) = review::render_engine(&line, width, engine.as_ref())?;
                human.write_all(rendered.as_bytes())?;
                corrected.write_all(record.corrected.as_bytes())?;
                corrected.write_all(line.ending.as_bytes())?;
                serde_json::to_writer(&mut debug, &record)?;
                writeln!(debug)?;
            }
            human.flush()?;
            debug.flush()?;
            corrected.flush()?;
            let mut info = manifest(&input, contents.as_bytes(), "review");
            info["engine"] = json!(engine.name());
            info["resources"] = metadata;
            save_json(&output_dir.join("manifest.json"), &info)?;
            println!("{} review: {}", engine.name(), output_dir.display());
        }
        Command::Benchmark {
            dataset,
            output_dir,
            engine,
        } => {
            let contents = fs::read(&dataset)?;
            let records = corpus::parse(
                std::str::from_utf8(&contents).context("corpus is not UTF-8")?,
                &dataset,
            )?;
            let (engine, metadata) = engine.load()?;
            let started = Instant::now();
            let run = metrics::benchmark(&records, engine.as_ref())?;
            let elapsed = started.elapsed();
            new_dir(&output_dir)?;
            save_json(&output_dir.join("metrics.json"), &run.report)?;
            let mut manifest = manifest(&dataset, &contents, "benchmark");
            manifest["evaluation_wall_ns"] = json!(elapsed.as_nanos());
            manifest["engine"] = json!(engine.name());
            manifest["resources"] = metadata;
            manifest["timing_scope"] = json!(
                "validation+correction+metrics+prediction collection; excludes resource load and serialization; NOT isolated correction latency"
            );
            save_json(&output_dir.join("manifest.json"), &manifest)?;
            let mut predictions = new_file(&output_dir.join("predictions.jsonl"))?;
            for record in &run.predictions {
                serde_json::to_writer(&mut predictions, record)?;
                writeln!(predictions)?;
            }
            predictions.flush()?;
            println!("{}", serde_json::to_string_pretty(&run.report)?);
        }
        Command::Inspect {
            text: source,
            engine,
        } => {
            let (engine, metadata) = engine.load()?;
            let result = analyze(&source, engine.as_ref());
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"source":source,"engine":engine.name(),"resources":metadata,
                        "tokens":result.tokens,"decisions":result.decisions,"corrected":result.corrected,"edits":result.edits})
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
