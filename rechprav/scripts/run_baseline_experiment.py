#!/usr/bin/env python3
"""Reproduce the synthetic KEEP/B1/protection ablations; never overwrite a run."""
import argparse
import csv
import hashlib
import json
import platform
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    binary = root / "target/release/rechprav"
    dataset = root / "data/fixtures/baseline/corpus.jsonl"
    output = args.output_dir.resolve()
    profiles = {
        "keep": None,
        "baseline": "baseline.toml",
        "no-domain": "baseline-no-domain.toml",
        "no-ignore": "baseline-no-ignore.toml",
        "allow-titlecase": "baseline-allow-titlecase.toml",
    }
    validation = subprocess.run(
        [str(binary), "validate", str(dataset)], check=True, capture_output=True, text=True
    )
    validated = json.loads(validation.stdout)
    if validated["real_asr"] or validated["pending"] or validated["ambiguous"]:
        raise ValueError("this experiment requires reviewed synthetic fixtures only")
    output.mkdir()  # Existing output is an error; results are immutable.
    (output / "validation.json").write_text(validation.stdout, encoding="utf-8")
    commands = []
    rows = []
    resource_manifests = {}
    dataset_hash = hashlib.sha256(dataset.read_bytes()).hexdigest()
    for name, config in profiles.items():
        command = [str(binary), "benchmark", str(dataset), "--output-dir", str(output / name)]
        if config:
            command += ["--config", str(root / "configs" / config)]
        commands.append(command)
        (output / "commands.json").write_text(
            json.dumps(commands, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        process = subprocess.run(command, capture_output=True, text=True)
        if process.returncode:
            (output / "failure.txt").write_text(process.stderr, encoding="utf-8")
            raise RuntimeError(f"{name}: {process.stderr}")
        m = json.loads((output / name / "metrics.json").read_text(encoding="utf-8"))
        manifest = json.loads((output / name / "manifest.json").read_text(encoding="utf-8"))
        resource_manifests[name] = manifest["resources"]
        if manifest["input_sha256"] != dataset_hash:
            raise ValueError("dataset changed during comparison")
        for file, expected_hash in manifest["compiled_source_sha256"].items():
            if hashlib.sha256((root / file).read_bytes()).hexdigest() != expected_hash:
                raise ValueError(f"binary was built from different source: {file}; rebuild release")
        counts = m["counts_1_to_1"]
        rows.append({
            "profile": name,
            "tp": counts["tp"], "fp": counts["fp"], "fn": counts["fn_"],
            "correct_input": counts["correct_input"], "harmed": counts["harmed_correct_input"],
            "precision": m["overall_precision"], "recall_1_to_1": m["rates_1_to_1"]["recall"],
            "recall_all_spans": m["all_gold_span_recall"],
            "f05_1_to_1": m["rates_1_to_1"]["f05"],
            "fcr": m["rates_1_to_1"]["false_correction_rate"],
            "wer_before": m["wer_before"], "wer_after": m["wer_after"],
            "candidate_recall_20": m["candidate_recall_at_k"]["20"],
        })
    with (output / "comparison.csv").open("x", encoding="utf-8", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    predictions = [json.loads(line) for line in
                   (output / "baseline/predictions.jsonl").read_text(encoding="utf-8").splitlines()]
    with (output / "errors.jsonl").open("x", encoding="utf-8") as file:
        for prediction in predictions:
            if prediction["prediction"] != prediction["gold"]:
                file.write(json.dumps(prediction, ensure_ascii=False) + "\n")
    git = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"], capture_output=True, text=True)
    status = subprocess.run(
        ["git", "-C", str(root), "status", "--porcelain", "--", "src", "tests", "scripts", "configs", "Cargo.toml", "Cargo.lock", "data/fixtures/baseline"],
        capture_output=True, text=True,
    )
    environment = {
        "observed_at_utc": datetime.now(timezone.utc).isoformat(),
        "git_revision": git.stdout.strip() if git.returncode == 0 else None,
        "implementation_worktree_status": status.stdout.splitlines(),
        "platform": platform.platform(),
        "dataset_sha256": dataset_hash,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "resources": resource_manifests,
        "scope": "synthetic mechanism checks; no real-ASR quality or performance claim",
    }
    for tool in ["rustc", "cargo"]:
        executable = shutil.which(tool)
        fallback = Path.home() / ".local/bin" / tool
        if not executable and fallback.exists():
            executable = str(fallback)
        if executable:
            check = subprocess.run([executable, "--version"], capture_output=True, text=True)
            environment[tool] = check.stdout.strip() if check.returncode == 0 else None
    if shutil.which("lscpu"):
        cpu = subprocess.run(["lscpu"], capture_output=True, text=True)
        environment["cpu"] = {
            line.split(":", 1)[0]: line.split(":", 1)[1].strip()
            for line in cpu.stdout.splitlines()
            if line.split(":", 1)[0] in ["Architecture", "CPU(s)", "Model name", "Hypervisor vendor"]
        }
    (output / "environment.json").write_text(
        json.dumps(environment, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(rows, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
