#!/usr/bin/env python3
"""011: explicit term families and independently replayed historical benchmarks."""
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


from family_profiles import profiles


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

    print("Replaying 106 historical benchmarks with two independent processes", flush=True)
    run([sys.executable, root / "scripts/replay_experiment.py", root / "experiments/010-user-phrases", output / "regression-010"])
    equal_profiles = json.loads((output / "regression-010/replay.json").read_text())["byte_equal_profiles"]
    assert equal_profiles == 106
    print("106 historical benchmarks reproduced byte-for-byte", flush=True)
    configs = profiles(root)
    (output / "configs").mkdir()
    for name, config in configs.items(): write_config(output / "configs" / f"{name}.toml", config)
    summary = []; breakdown = {}
    cohorts = {"user-reported": "data/user-examples/2026-09-21/corpus.jsonl", "synthetic-controls": "data/fixtures/user-phrases/corpus.jsonl", "new-families": "data/fixtures/term-families/corpus.jsonl"}
    for cohort, folder in cohorts.items():
        corpus = root / folder
        directory = output / cohort; directory.mkdir()
        validation = json.loads(run([binary, "validate", corpus]))
        assert validation["pending"] == validation["ambiguous"] == 0
        assert validation["real_asr"] == (validation["records"] if cohort == "user-reported" else 0)
        save(directory / "validation.json", validation)
        records = [json.loads(row) for row in corpus.read_text().splitlines()]
        outputs = {}; reports = {}; case_rows = [[r["id"], r["source"], r["gold"]] for r in records]
        for name, config in configs.items():
            config_path = output / "configs" / f"{name}.toml"
            run([binary, "terms-benchmark", corpus, "--config", config_path, "--output-dir", directory / name])
            manifest = json.loads((directory / name / "manifest.json").read_text())
            assert manifest["input_sha256"] == sha(corpus)
            assert manifest["resources"]["effective_config"] == config
            assert manifest["resources"]["config_sha256"] == sha(config_path)
            report = json.loads((directory / name / "metrics.json").read_text())
            predictions = [json.loads(row) for row in (directory / name / "predictions.jsonl").read_text().splitlines()]
            outputs[name] = predictions; reports[name] = report
        for left, right in zip(outputs["flat-complete"], outputs["family"], strict=True):
            assert left["id"] == right["id"]
            assert left["prediction"] == right["prediction"] and left["edits"] == right["edits"]
            assert left["comparisons"] == right["comparisons"]
        references = {name: "family" for name in configs}
        references["old-flat"] = "old-flat"; references["flat-complete"] = "old-flat"
        references["family"] = "flat-complete"; references["family-preferred"] = "family-conflict"
        for name in configs:
            predictions = outputs[name]; report = reports[name]
            reference = references[name]
            tp = fp = fn = harmed = gold_positions = 0; changes = []; errors = []; tags = {}
            for index, (record, prediction, old) in enumerate(zip(records, predictions, outputs[reference], strict=True)):
                assert record["id"] == prediction["id"] == old["id"]
                assert prediction["complete"]
                signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
                expected = {signature(e) for e in record["edits"]}
                proposed = {signature(e) for e in prediction["edits"]}
                earlier = {signature(e) for e in old["edits"]}
                counts = (len(expected & proposed), len(proposed - expected), len(expected - proposed))
                tp += counts[0]; fp += counts[1]; fn += counts[2]
                mask = lambda edits: {t for e in edits for t in range(e["token_start"], e["token_end"])}
                harmed += len(mask(prediction["edits"]) - mask(record["edits"]))
                gold_positions += len(mask(record["edits"]))
                source = record["source"].encode(); cursor = 0; parts = []
                for edit in prediction["edits"]:
                    assert cursor <= edit["byte_start"] < edit["byte_end"] <= len(source)
                    parts.extend([source[cursor:edit["byte_start"]], edit["replacement"].encode()]); cursor = edit["byte_end"]
                parts.append(source[cursor:]); assert b"".join(parts).decode() == prediction["prediction"]
                case_rows[index].append(prediction["prediction"])
                item = {"id": record["id"], "source": record["source"], "gold": record["gold"], "prediction": prediction["prediction"], "tags": record["tags"]}
                if record["gold"] != prediction["prediction"]: errors.append(item)
                if proposed != earlier:
                    changes.append({**item, "reference_profile": reference, "reference_prediction": old["prediction"],
                        "gained_tp": len((proposed - earlier) & expected), "lost_tp": len((earlier - proposed) & expected),
                        "removed_fp": len((earlier - expected) - proposed), "new_fp": len((proposed - expected) - earlier)})
                for tag in record["tags"]:
                    stats = tags.setdefault(tag, {"records": 0, "tp": 0, "fp": 0, "fn": 0})
                    stats["records"] += 1
                    for key, value in zip(["tp", "fp", "fn"], counts): stats[key] += value
            assert (tp, fp, fn, harmed) == tuple(report["span_counts"][k] for k in ["tp", "fp", "fn_", "harmed_correct_input"])
            assert report["span_counts"]["correct_input"] == report["source_tokens"] - gold_positions
            assert report["incomplete_records"] == 0
            save(directory / name / "errors.json", errors)
            save(directory / name / "changes-from-reference.json", changes)
            breakdown[f"{cohort}/{name}"] = tags
            row = {"corpus": cohort, "profile": name, "reference": reference, "tp": tp, "fp": fp, "fn": fn, "harmed_correct_input": harmed,
                   "exact_records": report["exact_records"], "wer_after": report["wer_after"],
                   "total_comparisons": sum(p["comparisons"] for p in predictions),
                   **{key: sum(c[key] for c in changes) for key in ["gained_tp", "lost_tp", "removed_fp", "new_fp"]}}
            summary.append(row); print(row, flush=True)
        with (directory / "case-comparison.csv").open("x", newline="", encoding="utf-8") as stream:
            writer = csv.writer(stream, lineterminator="\n"); writer.writerow(["id", "source", "gold", *configs]); writer.writerows(case_rows)

    # Verify resources and compiled code for every run, including historical controls.
    baseline_path = Path(configs["family"]["baseline_config"])
    baseline = tomllib.loads(baseline_path.read_text())
    resource_paths = {"config": baseline_path, **{key: baseline_path.parent / baseline[key] for key in ["dictionary", "domain", "ignore"]},
                      "frequency": baseline_path.parent / baseline["frequency"]["path"]}
    hashes = {key: sha(path) for key, path in resource_paths.items()}
    source_reports = resource_reports = 0
    for path in output.rglob("manifest.json"):
        manifest = json.loads(path.read_text())
        for file, digest in manifest["compiled_source_sha256"].items(): assert sha(root / file) == digest, file
        source_reports += 1
        resources = manifest["resources"]
        if "baseline_resources" in resources: resources = resources["baseline_resources"]
        if resources:
            for key, digest in resources["resource_sha256"].items():
                assert hashes[key] == digest, (path, key)
                resource_reports += 1
    assert source_reports == 142
    with (output / "comparison.csv").open("x", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0], lineterminator="\n"); writer.writeheader(); writer.writerows(summary)
    save(output / "summary.json", summary); save(output / "breakdown.json", breakdown); save(output / "commands.json", commands)
    save(output / "environment.json", {"implementation_commit": subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        "implementation_status": subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "configs", "data"], text=True).splitlines(),
        "platform": platform.platform(), "binary_sha256": sha(binary),
        "runner_sha256": {name: sha(root / "scripts" / name) for name in ["run_family_experiment.py", "family_profiles.py", "replay_experiment.py", "user_phrase_profiles.py", "run_term_experiment.py"]},
        "corpus_sha256": {name: sha(root / folder) for name, folder in cohorts.items()},
        "source_fingerprint_reports": source_reports, "resource_hashes_verified": resource_reports, "legacy_byte_equal_profiles": equal_profiles,
        "flat_config_sha256": sha(root / "configs/terms-user-phrases.toml"),
        "scope": "explicit forms, not morphology; family/flat output equality; three separate dev corpora; 106 historical profiles exactly reproduced"})


if __name__ == "__main__":
    main()
