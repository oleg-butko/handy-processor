//! Bounded JSONL adapter for Handy. The tested term algorithm is unchanged.
use anyhow::{Result, ensure};
use rechprav::terms::TermEngine;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: u64,
    text: String,
}

fn send(value: &Value) -> Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("config required"))?;
    let (engine, metadata) = TermEngine::load(std::path::Path::new(&path), false)?;
    let config = &metadata["effective_config"];
    let policy = &config["policy"];
    let profile = rechprav::hashing::sha256_hex(&serde_json::to_vec(&metadata)?);
    send(&json!({"ready":true,"protocol":1,"profile":profile}))?;
    let mut input = io::stdin().lock();
    loop {
        let mut line = Vec::new();
        if (&mut input).take(65537).read_until(b'\n', &mut line)? == 0 {
            break;
        }
        ensure!(
            line.len() <= 65536 && line.ends_with(b"\n"),
            "request line too long"
        );
        let request: Request = serde_json::from_slice(&line)?;
        ensure!(request.text.len() <= 16384, "text too long");
        let analysis = engine.analyze(&request.text);
        let mut decisions = Vec::new();
        let mut omitted_candidates = 0;
        // Record selected spans first when the trace limit is reached.
        let mut ordered: Vec<_> = analysis.decisions.iter().collect();
        ordered.sort_by_key(|d| (d.selected.is_none(), d.byte_start));
        for d in ordered.iter().take(32) {
            let mut value = serde_json::to_value(d)?;
            let mut candidates = Vec::new();
            for c in d.candidates.iter().take(6) {
                let set = config["sets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["id"] == c.set)
                    .unwrap();
                let term = set["terms"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["id"] == c.term)
                    .unwrap();
                let context = !c.context_hits.is_empty() || !c.context_pattern_hits.is_empty();
                let parts = json!({
                    "base":4.0, "global":policy["boost"], "set":set["boost"], "term":term["boost"],
                    "distance":-policy["edit_penalty"].as_f64().unwrap() * c.weighted_cost.unwrap_or(c.distance as f64),
                    "context":if context { policy["context_bonus"].as_f64().unwrap() } else { 0.0 },
                    "known":if c.contains_known_word { -policy["known_penalty"].as_f64().unwrap() } else { 0.0 },
                    "source":c.source_penalty,
                });
                let sum: f64 = parts
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|v| v.as_f64().unwrap())
                    .sum();
                ensure!((sum - c.score).abs() < 1e-8, "score explanation mismatch");
                let mut candidate = serde_json::to_value(c)?;
                candidate["parts"] = parts;
                candidates.push(candidate);
            }
            omitted_candidates += d.candidates.len().saturating_sub(candidates.len());
            value["candidates"] = json!(candidates);
            decisions.push(value);
        }
        send(&json!({"id":request.id,"text":analysis.corrected,"trace":{
            "schemaVersion":1,"profile":profile,"complete":analysis.complete,
            "minScore":policy["min_score"],"minMargin":policy["min_margin"],
            "decisions":decisions,"omittedDecisions":ordered.len().saturating_sub(32),
            "omittedCandidates":omitted_candidates,
        }}))?;
    }
    Ok(())
}
