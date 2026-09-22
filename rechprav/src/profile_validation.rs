//! Replay validation shared by offline publication and the in-memory reader.
use crate::{feedback, feedback_composition, feedback_registry};
use anyhow::{Context, Result, ensure};
use rechprav::{
    corrector::Baseline,
    hashing::sha256_hex,
    terms::{Config, TermEngine},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    manual_path: PathBuf,
    manual: String,
    events: String,
    decisions: String,
    bindings: String,
    report: String,
    config: String,
    manifest: String,
}
fn jsonl<T: serde::de::DeserializeOwned>(text: &str) -> Result<Vec<T>> {
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line).with_context(|| format!("JSONL line {}", i + 1))
        })
        .collect()
}
fn check_hash(expected: &Value, contents: &str, name: &str) -> Result<()> {
    ensure!(
        expected.as_str() == Some(&sha256_hex(contents.as_bytes())),
        "{name} hash mismatch"
    );
    Ok(())
}
pub fn validate(bytes: &[u8]) -> Result<(TermEngine, Value)> {
    ensure!(bytes.len() <= 8 * 1024 * 1024, "bundle exceeds 8 MiB");
    let bundle: Bundle = serde_json::from_slice(bytes)?;
    let manifest: Value = serde_json::from_str(&bundle.manifest)?;
    let compiled_sources: std::collections::BTreeMap<_, _> = [
        (
            "examples/feedback_compose.rs",
            include_bytes!("../examples/feedback_compose.rs").as_slice(),
        ),
        (
            "src/feedback_registry.rs",
            include_bytes!("feedback_registry.rs").as_slice(),
        ),
        (
            "src/feedback_composition.rs",
            include_bytes!("feedback_composition.rs").as_slice(),
        ),
        ("Cargo.toml", include_bytes!("../Cargo.toml").as_slice()),
        ("Cargo.lock", include_bytes!("../Cargo.lock").as_slice()),
        ("src/lib.rs", include_bytes!("lib.rs").as_slice()),
        ("src/hashing.rs", include_bytes!("hashing.rs").as_slice()),
        ("src/corpus.rs", include_bytes!("corpus.rs").as_slice()),
        (
            "src/corrector.rs",
            include_bytes!("corrector.rs").as_slice(),
        ),
        (
            "src/dictionary.rs",
            include_bytes!("dictionary.rs").as_slice(),
        ),
        (
            "src/frequency.rs",
            include_bytes!("frequency.rs").as_slice(),
        ),
        ("src/metrics.rs", include_bytes!("metrics.rs").as_slice()),
        ("src/review.rs", include_bytes!("review.rs").as_slice()),
        ("src/text.rs", include_bytes!("text.rs").as_slice()),
        ("src/terms.rs", include_bytes!("terms.rs").as_slice()),
        (
            "src/pronunciation.rs",
            include_bytes!("pronunciation.rs").as_slice(),
        ),
        (
            "src/term_context.rs",
            include_bytes!("term_context.rs").as_slice(),
        ),
        (
            "src/term_metrics.rs",
            include_bytes!("term_metrics.rs").as_slice(),
        ),
        (
            "src/packed_frequency.rs",
            include_bytes!("packed_frequency.rs").as_slice(),
        ),
        ("src/packed.rs", include_bytes!("packed.rs").as_slice()),
        ("src/feedback.rs", include_bytes!("feedback.rs").as_slice()),
    ]
    .into_iter()
    .map(|(name, bytes)| (name, rechprav::hashing::sha256_hex(bytes)))
    .collect();

    ensure!(
        manifest["compiled_source_sha256"] == serde_json::to_value(&compiled_sources)?,
        "producer compiled sources mismatch"
    );
    for (field, contents) in [
        ("input_sha256", &bundle.events),
        ("decisions_sha256", &bundle.decisions),
        ("bindings_sha256", &bundle.bindings),
        ("reference_config_sha256", &bundle.manual),
    ] {
        check_hash(&manifest[field], contents, field)?;
    }
    check_hash(
        &manifest["output_sha256"]["report.json"],
        &bundle.report,
        "report",
    )?;
    check_hash(
        &manifest["output_sha256"]["composed.toml"],
        &bundle.config,
        "config",
    )?;
    ensure!(
        bundle.manual_path.is_absolute(),
        "manual_path must be absolute"
    );
    let mut manual: Config = toml::from_str(&bundle.manual)?;
    manual.validate()?;
    manual.baseline_config = bundle
        .manual_path
        .parent()
        .unwrap_or(Path::new("/"))
        .join(&manual.baseline_config)
        .canonicalize()?;
    let (base, resources) = Baseline::load(&manual.baseline_config)?;
    ensure!(
        manifest["resources"] == resources,
        "baseline resources mismatch"
    );
    let environment = sha256_hex(&serde_json::to_vec(&json!({
        "compiled_sources":compiled_sources, "resources":resources,
        "bindings_sha256":sha256_hex(bundle.bindings.as_bytes()),
        "reference_config_sha256":sha256_hex(bundle.manual.as_bytes())
    }))?);
    ensure!(
        manifest["environment_sha256"].as_str() == Some(&environment),
        "environment mismatch"
    );
    let scope = manifest["scope"].as_str().context("missing scope")?;
    let events: Vec<feedback::Event> = jsonl(&bundle.events)?;
    let decisions: Vec<feedback_registry::Decision> = jsonl(&bundle.decisions)?;
    let settings: feedback_composition::Settings = serde_json::from_str(&bundle.bindings)?;
    let registry =
        feedback_registry::rebuild(&events, &decisions, scope, &manual, &base, &environment)?;
    let rebuilt = feedback_composition::compose(&manual, registry, &settings, base.strict_yo)?;
    let report: Value = serde_json::from_str(&bundle.report)?;
    ensure!(
        report == serde_json::to_value(&rebuilt)?,
        "report differs from replay"
    );
    let config: Config = toml::from_str(&bundle.config)?;
    config.validate()?;
    ensure!(
        serde_json::to_value(&config)? == serde_json::to_value(&rebuilt.config)?,
        "config differs from replay"
    );
    let engine = TermEngine::new(config, Some(base))?;
    let metadata = json!({"schema_version":1,"scope":scope,
        "bundle_sha256":sha256_hex(bytes),
        "config_sha256":sha256_hex(bundle.config.as_bytes()),
        "environment_sha256":environment,
        "validator_source_sha256":sha256_hex(include_bytes!("../examples/profile_validate.rs")),
        "validation_source_sha256":sha256_hex(include_bytes!("profile_validation.rs")),
        "compiled_source_sha256":compiled_sources,
        "resources":resources});
    Ok((engine, metadata))
}
