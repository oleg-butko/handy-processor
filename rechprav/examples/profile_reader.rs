//! Offline JSONL driver for the single-thread engine lifetime experiment.
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
use anyhow::{Context, Result, ensure};
use profile_reader::{LoadedProfile, Reader, Snapshot, prepare, valid_id};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Read, Write},
    rc::Rc,
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Reload { snapshot: Snapshot },
    Acquire { id: String },
    Analyze { id: String, text: String },
    Release { id: String },
    Status,
}
fn execute(
    reader: &mut Reader,
    handles: &mut BTreeMap<String, Rc<LoadedProfile>>,
    command: Command,
) -> Result<Value> {
    Ok(match command {
        Command::Reload { snapshot } => {
            let changed = match reader.observe(snapshot)? {
                Some(ticket) => reader.install(prepare(ticket)?)?,
                None => false,
            };
            json!({"changed":changed})
        }
        Command::Acquire { id } => {
            valid_id(&id)?;
            ensure!(
                !handles.contains_key(&id) && handles.len() < 16,
                "duplicate handle or handle limit"
            );
            handles.insert(id, reader.acquire()?);
            json!({})
        }
        Command::Analyze { id, text } => {
            ensure!(text.len() <= 64 * 1024, "text exceeds 64 KiB");
            serde_json::to_value(handles.get(&id).context("unknown handle")?.analyze(&text))?
        }
        Command::Release { id } => {
            ensure!(handles.remove(&id).is_some(), "unknown handle");
            json!({})
        }
        Command::Status => reader.status(),
    })
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 2,
        "usage: profile_reader SCOPE < commands.jsonl"
    );
    let mut reader = Reader::new(&args[1])?;
    let mut handles = BTreeMap::new();
    let compiled_reader = json!({
        "examples/profile_reader.rs":rechprav::hashing::sha256_hex(include_bytes!("profile_reader.rs")),
        "src/profile_reader.rs":rechprav::hashing::sha256_hex(include_bytes!("../src/profile_reader.rs")),
        "src/profile_validation.rs":rechprav::hashing::sha256_hex(include_bytes!("../src/profile_validation.rs"))
    });
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take(10 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        ensure!(read <= 10 * 1024 * 1024, "command exceeds 10 MiB");
        let result = serde_json::from_slice(&line)
            .context("parse command")
            .and_then(|command| execute(&mut reader, &mut handles, command));
        let response = match result {
            Ok(output) => json!({"ok":true,"output":output,"status":reader.status()}),
            Err(error) => json!({"ok":false,"error":format!("{error:#}"),"status":reader.status()}),
        };
        println!(
            "{}",
            json!({"result":response,"compiled_source_sha256":compiled_reader})
        );
        io::stdout().flush()?;
    }
    Ok(())
}
