#!/usr/bin/env python3
"""Stage 007: fixed known-word/context ablations, separate old and new corpora."""
import argparse
import copy
import csv
import json
import platform
import subprocess
import sys
import tomllib
from pathlib import Path
from run_term_experiment import save, sha, write_config


def profiles(root):
    source = root / "configs/terms-example.toml"
    base = tomllib.loads(source.read_text())
    base["baseline_config"] = str((source.parent / base["baseline_config"]).resolve())
    policy = json.loads((root / "data/fixtures/term-context/policy.json").read_text())
    result = {}
    for name in ["legacy", "known-exact", "phrases", "combined", "exclusions", "required", "legacy-conflict", "routed", "routed-no-exclusions"]:
        config = copy.deepcopy(base)
        use_phrases = name in ["phrases", "combined", "exclusions", "required", "routed", "routed-no-exclusions"]
        cap_known = name in ["known-exact", "combined", "exclusions", "required", "routed", "routed-no-exclusions"]
        exclude = name in ["exclusions", "required", "routed"]
        require = name in ["required", "routed", "routed-no-exclusions"]
        if name in ["legacy-conflict", "routed", "routed-no-exclusions"]:
            config["sets"][1]["enabled"] = True
        for group in config["sets"]:
            for term in group["terms"]:
                rule = policy[group["id"] + "/" + term["id"]]
                if cap_known: term["max_known_distance"] = 0
                if use_phrases:
                    term["context"] = []
                    term["context_patterns"] = rule["context_patterns"]
                if exclude and rule["context_exclusions"]:
                    term["context_exclusions"] = rule["context_exclusions"]
                if require: term["require_context"] = True
        result[name] = config
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = root / "target/release/rechprav"
    output = args.output_dir.resolve(); output.mkdir()
    commands = []

    def run(command):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            save(output / "failure.json", {"command": command, "stderr": result.stderr})
            result.check_returncode()
        return result.stdout

    # Preserve all 12 old configurations including byte-for-byte diagnostics, not just scores.
    run([sys.executable, root / "scripts/run_term_experiment.py", output / "legacy-regression"])
    for old in (root / "experiments/006-terms").glob("*/predictions.jsonl"):
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (old.parent / filename).read_bytes() == (output / "legacy-regression" / old.parent.name / filename).read_bytes(), old
    configs = profiles(root)
    (output / "configs").mkdir()
    for name, config in configs.items(): write_config(output / "configs" / f"{name}.toml", config)
    summary = []; breakdown = {}; source_reports = 0
    for corpus_name, corpus in [("old-006", root / "data/fixtures/terms/corpus.jsonl"), ("new-007", root / "data/fixtures/term-context/corpus.jsonl")]:
        directory = output / corpus_name; directory.mkdir()
        validation = json.loads(run([binary, "validate", corpus]))
        assert validation["real_asr"] == validation["pending"] == validation["ambiguous"] == 0
        save(directory / "validation.json", validation)
        records = [json.loads(row) for row in corpus.read_text().splitlines()]
        previous = None; case_rows = [[r["id"], r["source"], r["gold"]] for r in records]
        for name, config in configs.items():
            config_path = output / "configs" / f"{name}.toml"
            run([binary, "terms-benchmark", corpus, "--config", config_path, "--output-dir", directory / name])
            manifest = json.loads((directory / name / "manifest.json").read_text())
            for file, digest in manifest["compiled_source_sha256"].items(): assert sha(root / file) == digest, file
            assert manifest["input_sha256"] == sha(corpus)
            assert manifest["resources"]["effective_config"] == config
            assert manifest["resources"]["config_sha256"] == sha(config_path)
            source_reports += 1
            report = json.loads((directory / name / "metrics.json").read_text())
            predictions = [json.loads(row) for row in (directory / name / "predictions.jsonl").read_text().splitlines()]
            if name == "legacy": previous = predictions
            tp = fp = fn = 0; changes = []; errors = []; tags = {}
            for index, (record, prediction, old) in enumerate(zip(records, predictions, previous, strict=True)):
                assert record["id"] == prediction["id"] == old["id"]
                signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
                expected = {signature(e) for e in record["edits"]}
                proposed = {signature(e) for e in prediction["edits"]}
                earlier = {signature(e) for e in old["edits"]}
                counts = (len(expected & proposed), len(proposed - expected), len(expected - proposed))
                tp += counts[0]; fp += counts[1]; fn += counts[2]
                source = record["source"].encode(); cursor = 0; parts = []
                for edit in prediction["edits"]:
                    assert cursor <= edit["byte_start"] < edit["byte_end"] <= len(source)
                    parts.extend([source[cursor:edit["byte_start"]], edit["replacement"].encode()]); cursor = edit["byte_end"]
                parts.append(source[cursor:]); assert b"".join(parts).decode() == prediction["prediction"]
                case_rows[index].append(prediction["prediction"])
                item = {"id": record["id"], "source": record["source"], "gold": record["gold"], "prediction": prediction["prediction"], "tags": record["tags"]}
                if record["gold"] != prediction["prediction"]: errors.append(item)
                if proposed != earlier:
                    changes.append({**item, "legacy_prediction": old["prediction"],
                        "gained_tp": len((proposed - earlier) & expected), "lost_tp": len((earlier - proposed) & expected),
                        "removed_fp": len((earlier - expected) - proposed), "new_fp": len((proposed - expected) - earlier)})
                for tag in record["tags"]:
                    stats = tags.setdefault(tag, {"records": 0, "tp": 0, "fp": 0, "fn": 0})
                    stats["records"] += 1
                    for key, value in zip(["tp", "fp", "fn"], counts): stats[key] += value
            assert (tp, fp, fn) == tuple(report["span_counts"][k] for k in ["tp", "fp", "fn_"])
            assert report["incomplete_records"] == 0
            save(directory / name / "errors.json", errors)
            save(directory / name / "changes-from-legacy.json", changes)
            breakdown[f"{corpus_name}/{name}"] = tags
            row = {"corpus": corpus_name, "profile": name, "tp": tp, "fp": fp, "fn": fn,
                   "harmed_correct_input": report["span_counts"]["harmed_correct_input"], "exact_records": report["exact_records"], "wer_after": report["wer_after"]}
            summary.append(row); print(row, flush=True)
        with (directory / "case-comparison.csv").open("x", newline="") as stream:
            writer = csv.writer(stream, lineterminator="\n"); writer.writerow(["id", "source", "gold", *configs]); writer.writerows(case_rows)
    with (output / "comparison.csv").open("x", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0], lineterminator="\n"); writer.writeheader(); writer.writerows(summary)
    save(output / "summary.json", summary); save(output / "breakdown.json", breakdown)
    save(output / "commands.json", commands)
    save(output / "environment.json", {"implementation_commit": subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        "implementation_status": subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "configs", "data"], text=True).splitlines(),
        "platform": platform.platform(), "binary_sha256": sha(binary), "runner_sha256": sha(Path(__file__)), "shared_runner_sha256": sha(root / "scripts/run_term_experiment.py"),
        "old_corpus_sha256": sha(root / "data/fixtures/terms/corpus.jsonl"), "new_corpus_sha256": sha(root / "data/fixtures/term-context/corpus.jsonl"),
        "context_policy_sha256": sha(root / "data/fixtures/term-context/policy.json"), "source_fingerprint_reports": source_reports,
        "legacy_byte_equal_profiles": 14, "scope": "two separate authored synthetic corpora, nine frozen policies, old scores/aliases unchanged; exact 006 regression incl diagnostics; no independent ASR holdout"})


if __name__ == "__main__":
    main()
