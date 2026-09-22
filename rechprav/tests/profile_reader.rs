#[path = "../src/feedback.rs"]
mod feedback;
#[path = "../src/feedback_composition.rs"]
mod feedback_composition;
#[path = "../src/feedback_registry.rs"]
mod feedback_registry;
#[path = "../src/profile_reader.rs"]
mod profile_reader;
#[path = "../src/profile_validation.rs"]
mod profile_validation;
use profile_reader::{Reader, Snapshot, prepare};
use rechprav::hashing::sha256_hex;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, rc::Rc};

fn snapshot(dir: &Path, enabled: bool, generation: u64) -> Snapshot {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("dictionary.txt"), "слот\nсвод\n").unwrap();
    fs::write(dir.join("baseline.toml"), "dictionary = 'dictionary.txt'\n").unwrap();
    fs::write(
        dir.join("manual.toml"),
        format!(
            r#"baseline_config = "baseline.toml"
enabled = {enabled}
[[sets]]
id = "hardware"
enabled = true
[[sets.terms]]
id = "slot"
canonical = "слот"
aliases = ["свод"]
enabled = true
known_words = "always"
max_known_distance = 0
"#
        ),
    )
    .unwrap();
    fs::write(dir.join("events.jsonl"), "").unwrap();
    fs::write(dir.join("decisions.jsonl"), "").unwrap();
    fs::write(
        dir.join("bindings.json"),
        r#"{"enabled":true,"bindings":[]}"#,
    )
    .unwrap();
    let output = dir.join("composed");
    let result = Command::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/examples/feedback_compose"),
    )
    .arg(dir.join("events.jsonl"))
    .arg("--config")
    .arg(dir.join("manual.toml"))
    .arg("--decisions")
    .arg(dir.join("decisions.jsonl"))
    .arg("--bindings")
    .arg(dir.join("bindings.json"))
    .args(["--scope", "technical", "--output-dir"])
    .arg(&output)
    .output()
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let read = |p| fs::read_to_string(p).unwrap();
    let bundle = json!({"manual_path":dir.join("manual.toml"),"manual":read(dir.join("manual.toml")),
        "events":"","decisions":"","bindings":read(dir.join("bindings.json")),
        "config":read(output.join("composed.toml")),"report":read(output.join("report.json")),
        "manifest":read(output.join("composition-manifest.json"))});
    let bytes = serde_json::to_vec(&bundle).unwrap();
    // The actual publication validation, not a fake receipt for arbitrary rules.
    let (_, validation) = profile_validation::validate(&bytes).unwrap();
    serde_json::from_value(json!({"scope":"technical","desired_revision":generation,
        "active":{"generation":generation,"request_id":format!("r{generation}"),
            "bundle_sha256":sha256_hex(&bytes),"bundle":bundle,"validation":validation,
            "receipt":{"generation":generation,"request_id":format!("r{generation}"),
                "bundle_sha256":sha256_hex(&bytes),"config_sha256":validation["config_sha256"]}}}))
    .unwrap()
}
fn load(reader: &mut Reader, snapshot: Snapshot) {
    let ticket = reader.observe(snapshot).unwrap().unwrap();
    assert!(reader.install(prepare(ticket).unwrap()).unwrap());
}

#[test]
fn old_handle_finishes_on_old_engine_and_drops_after_last_use() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let b = snapshot(&dir.path().join("b"), false, 2);
    let mut reader = Reader::new("technical").unwrap();
    load(&mut reader, a);
    let old = reader.acquire().unwrap();
    let weak = Rc::downgrade(&old);
    assert_eq!(old.analyze("свод").analysis.corrected, "слот");
    load(&mut reader, b.clone());
    assert_eq!(old.analyze("свод").generation, 1);
    assert_eq!(old.analyze("свод").analysis.corrected, "слот");
    let new = reader.acquire().unwrap();
    assert_eq!(new.analyze("свод").generation, 2);
    assert_eq!(new.analyze("свод").analysis.corrected, "свод");
    assert!(reader.observe(b).unwrap().is_none());
    assert!(Rc::ptr_eq(&new, &reader.acquire().unwrap()));
    drop(old);
    assert!(weak.upgrade().is_none());
}

#[test]
fn late_prepared_engine_cannot_replace_a_newer_target() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let b = snapshot(&dir.path().join("b"), false, 2);
    let mut reader = Reader::new("technical").unwrap();
    let old = prepare(reader.observe(a).unwrap().unwrap()).unwrap();
    let new = prepare(reader.observe(b).unwrap().unwrap()).unwrap();
    assert!(reader.install(new).unwrap());
    assert!(
        reader
            .install(old)
            .unwrap_err()
            .to_string()
            .contains("superseded")
    );
    assert_eq!(reader.acquire().unwrap().analyze("свод").generation, 2);
}

#[test]
fn resource_failure_keeps_old_engine_but_invalidates_older_tickets() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let b = snapshot(&dir.path().join("b"), false, 2);
    let c = snapshot(&dir.path().join("c"), false, 3);
    let mut reader = Reader::new("technical").unwrap();
    load(&mut reader, a);
    let old_ticket = prepare(reader.observe(b).unwrap().unwrap()).unwrap();
    fs::write(dir.path().join("c/dictionary.txt"), "ввод\n").unwrap();
    assert!(prepare(reader.observe(c.clone()).unwrap().unwrap()).is_err());
    assert!(reader.install(old_ticket).is_err());
    assert_eq!(reader.acquire().unwrap().analyze("свод").generation, 1);
    assert_eq!(reader.status()["target_generation"], 3);
    fs::write(dir.path().join("c/dictionary.txt"), "слот\nсвод\n").unwrap();
    load(&mut reader, c);
    assert_eq!(reader.acquire().unwrap().analyze("свод").generation, 3);
}

#[test]
fn loaded_resources_are_owned_and_not_reopened_for_texts_or_duplicate_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let mut reader = Reader::new("technical").unwrap();
    load(&mut reader, a.clone());
    fs::remove_file(dir.path().join("a/dictionary.txt")).unwrap();
    fs::remove_file(dir.path().join("a/baseline.toml")).unwrap();
    assert!(reader.observe(a.clone()).unwrap().is_none());
    assert_eq!(
        reader.acquire().unwrap().analyze("свод").analysis.corrected,
        "слот"
    );
    let mut restarted = Reader::new("technical").unwrap();
    assert!(prepare(restarted.observe(a).unwrap().unwrap()).is_err());
    assert!(restarted.acquire().is_err());
}

#[test]
fn regressions_scope_and_same_generation_different_bytes_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 2);
    let b = snapshot(&dir.path().join("b"), false, 2);
    let mut reader = Reader::new("technical").unwrap();
    load(&mut reader, a.clone());
    assert!(reader.observe(b).is_err());
    let mut lower = a.clone();
    lower.desired_revision = 1;
    assert!(reader.observe(lower).is_err());
    let mut other = a.clone();
    other.scope = "other".into();
    assert!(reader.observe(other).is_err());
    let empty: Snapshot =
        serde_json::from_value(json!({"scope":"technical","desired_revision":3,"active":null}))
            .unwrap();
    assert!(reader.observe(empty).is_err());
    assert_eq!(reader.acquire().unwrap().analyze("свод").generation, 2);
}

#[test]
fn unready_reader_is_explicit_and_invalid_receipts_do_not_advance_it() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let mut reader = Reader::new("technical").unwrap();
    let empty: Snapshot =
        serde_json::from_value(json!({"scope":"technical","desired_revision":0,"active":null}))
            .unwrap();
    assert!(reader.observe(empty).unwrap().is_none());
    assert!(reader.acquire().is_err());
    let mut bad = a.clone();
    bad.active.as_mut().unwrap().receipt["generation"] = json!(9);
    assert!(reader.observe(bad).is_err());
    let mut bad = a.clone();
    bad.active.as_mut().unwrap().bundle["config"] = Value::String("broken".into());
    assert!(reader.observe(bad).is_err());
    assert_eq!(reader.status()["desired_revision"], 0);
    fs::remove_file(dir.path().join("a/dictionary.txt")).unwrap();
    assert!(prepare(reader.observe(a).unwrap().unwrap()).is_err());
    assert!(reader.acquire().is_err());
}

#[test]
fn pending_new_intent_cancels_load_without_replacing_an_existing_engine() {
    let dir = tempfile::tempdir().unwrap();
    let a = snapshot(&dir.path().join("a"), true, 1);
    let mut pending = a.clone();
    pending.desired_revision = 2;
    let mut reader = Reader::new("technical").unwrap();
    let old = prepare(reader.observe(a).unwrap().unwrap()).unwrap();
    let fresh = reader.observe(pending.clone()).unwrap().unwrap();
    assert!(reader.install(old).is_err());
    reader.install(prepare(fresh).unwrap()).unwrap();
    pending.desired_revision = 3;
    assert!(reader.observe(pending).unwrap().is_none());
    assert_eq!(reader.acquire().unwrap().analyze("свод").generation, 1);
}
