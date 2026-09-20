#!/usr/bin/env python3
"""Fixed synthetic ablations for optional term sets, real words and 1..3-word spans."""
import argparse
import copy
import csv
import hashlib
import json
import platform
import subprocess
import tomllib
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2); stream.write("\n")


def write_config(path, config):
    # All values here are JSON-compatible TOML scalars/arrays; no arbitrary mappings.
    emit = lambda key, value: f"{key} = {json.dumps(value, ensure_ascii=False)}\n"
    with path.open("x", encoding="utf-8") as stream:
        stream.write(emit("baseline_config", config["baseline_config"]) + emit("enabled", config["enabled"]))
        stream.write("\n[policy]\n")
        for key, value in config["policy"].items(): stream.write(emit(key, value))
        for group in config["sets"]:
            stream.write("\n[[sets]]\n")
            for key, value in group.items():
                if key != "terms": stream.write(emit(key, value))
            for term in group["terms"]:
                stream.write("\n[[sets.terms]]\n")
                for key, value in term.items(): stream.write(emit(key, value))
    assert tomllib.loads(path.read_text()) == config


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = root / "target/release/rechprav"
    corpus = root / "data/fixtures/terms/corpus.jsonl"
    control = root / "data/fixtures/baseline/corpus.jsonl"
    original = root / "configs/terms-example.toml"
    base = tomllib.loads(original.read_text())
    base["baseline_config"] = str((original.parent / base["baseline_config"]).resolve())
    profiles = {}
    for name in ["disabled", "exact", "edit-1", "edit-2", "oov-only", "unguarded", "pnz-low", "pnz-high", "conflict", "prefer-hardware", "lab-only", "pnz-disabled"]:
        c = copy.deepcopy(base)
        pnz = c["sets"][0]["terms"][0]
        if name == "disabled": c["enabled"] = False
        if name == "exact": c["policy"]["mode"] = "exact"
        if name == "edit-2": c["policy"]["max_distance"] = 2
        if name in ["oov-only", "unguarded"]:
            for group in c["sets"]:
                for term in group["terms"]: term["known_words"] = "never" if name == "oov-only" else "always"
        if name == "pnz-low": pnz["boost"] = -2.0
        if name == "pnz-high": pnz["boost"] = 2.0
        if name in ["conflict", "prefer-hardware", "lab-only"]: c["sets"][1]["enabled"] = True
        if name == "prefer-hardware": c["sets"][0]["boost"] = 1.0
        if name == "lab-only": c["sets"][0]["enabled"] = False
        if name == "pnz-disabled": pnz["enabled"] = False
        profiles[name] = c
    output = args.output_dir.resolve(); output.mkdir()
    commands = []

    def run(command):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            save(output / "failure.json", {"command": command, "stderr": result.stderr})
            result.check_returncode()
        return result.stdout

    def verify_manifest(path):
        manifest = json.loads(path.read_text())
        for name, expected in manifest["compiled_source_sha256"].items():
            assert sha(root / name) == expected, f"stale binary: {name}"
        return manifest

    validation = json.loads(run([binary, "validate", corpus]))
    assert validation["real_asr"] == validation["pending"] == validation["ambiguous"] == 0
    save(output / "validation.json", validation)
    records = [json.loads(row) for row in corpus.read_text().splitlines()]
    summary = []; outputs = {}
    for name, config in profiles.items():
        config_path = output / f"{name}.toml"; write_config(config_path, config)
        run([binary, "terms-benchmark", corpus, "--config", config_path, "--output-dir", output / name])
        manifest = verify_manifest(output / name / "manifest.json")
        assert manifest["input_sha256"] == sha(corpus)
        assert manifest["resources"]["effective_config"] == config
        assert manifest["resources"]["config_sha256"] == sha(config_path)
        report = json.loads((output / name / "metrics.json").read_text())
        predictions = [json.loads(row) for row in (output / name / "predictions.jsonl").read_text().splitlines()]
        outputs[name] = predictions
        tp = fp = fn = harmed = correct = 0
        errors = []
        for record, prediction in zip(records, predictions, strict=True):
            assert record["id"] == prediction["id"]
            signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
            proposed = {signature(e) for e in prediction["edits"]}; expected = {signature(e) for e in record["edits"]}
            tp += len(proposed & expected); fp += len(proposed - expected); fn += len(expected - proposed)
            # Independently rebuild bytes to verify span mapping and preservation.
            source = record["source"].encode(); cursor = 0; parts = []
            for edit in prediction["edits"]:
                assert cursor <= edit["byte_start"] < edit["byte_end"] <= len(source)
                parts.extend([source[cursor:edit["byte_start"]], edit["replacement"].encode()]); cursor = edit["byte_end"]
            parts.append(source[cursor:]); assert b"".join(parts).decode() == prediction["prediction"]
            if prediction["prediction"] != record["gold"]:
                errors.append({"id": record["id"], "tags": record["tags"], "source": record["source"], "gold": record["gold"], "prediction": prediction["prediction"], "fp": len(proposed - expected), "fn": len(expected - proposed)})
        assert (tp, fp, fn) == tuple(report["span_counts"][k] for k in ["tp", "fp", "fn_"])
        assert report["incomplete_records"] == 0
        save(output / name / "errors.json", errors)
        row = {"profile": name, "tp": tp, "fp": fp, "fn": fn, "harmed_correct_input": report["span_counts"]["harmed_correct_input"],
               "exact_records": report["exact_records"], "wer_after": report["wer_after"]}
        summary.append(row); print(row, flush=True)
    assert all(p["prediction"] == p["source"] and not p["edits"] for p in outputs["disabled"])
    # B2 stays independent: no term pass or cascading hidden inside this control.
    for name, dataset in [("b2-old", control), ("b2-new", corpus)]:
        run([binary, "benchmark", dataset, "--config", base["baseline_config"], "--output-dir", output / name])
        verify_manifest(output / name / "manifest.json")
    for file in ["metrics.json", "predictions.jsonl"]:
        assert (output / "b2-old" / file).read_bytes() == (root / "experiments/003-frequency/b2-full" / file).read_bytes()
    with (output / "comparison.csv").open("x", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0].keys()); writer.writeheader(); writer.writerows(summary)
    save(output / "summary.json", summary)
    save(output / "commands.json", commands)
    save(output / "environment.json", {"implementation_commit": subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        "implementation_status": subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "configs", "data/fixtures/terms"], text=True).splitlines(),
        "platform": platform.platform(), "binary_sha256": sha(binary), "runner_sha256": sha(Path(__file__)), "corpus_sha256": sha(corpus),
        "prototype_config_sha256": sha(original), "scope": "fixed synthetic stress corpus; 12 a priori profiles; same weights except specified ablation; no real ASR quality claim; full B2 unchanged; config snapshots contain resolved baseline path"})


if __name__ == "__main__":
    main()
