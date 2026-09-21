#!/usr/bin/env python3
"""014: editor journals to disabled drafts, explicit simulation of acceptance, exact controls."""
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


from draft_profiles import scenarios


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

    configs = {}; draft_reports = {}
    (output / "configs").mkdir(); (output / "events").mkdir(); (output / "drafts").mkdir()
    reference_config = root / "configs/terms-families.toml"
    draft_binary = root / "target/release/examples/feedback_draft"
    for name, (events, scope) in scenarios().items():
        events_path = output / "events" / f"{name}.jsonl"
        with events_path.open("x") as stream:
            for event in events: stream.write(json.dumps(event,ensure_ascii=False)+"\n")
        run([draft_binary, events_path, "--config", reference_config, "--scope", scope, "--output-dir", output / "drafts" / name])
        draft_dir = output / "drafts" / name
        report = json.loads((draft_dir / "report.json").read_text()); draft_reports[name] = report
        config = tomllib.loads((draft_dir / "proposals.toml").read_text())
        assert config == report["config"]
        assert not config["enabled"] and all(not g["enabled"] and all(not t["enabled"] for t in g["terms"]) for g in config["sets"])
        if name != "disabled":
            config["enabled"] = True
            for group in config["sets"]:
                group["enabled"] = True
                for term in group["terms"]:
                    term["enabled"] = True
                    if name == "no-context":
                        term.pop("context_patterns", None); term.pop("require_context", None)
        configs[name] = config
        write_config(output / "configs" / f"{name}.toml", config)
    for name in ["duplicate", "repeated", "undo-one"]: assert configs[name] == configs["accepted"]
    assert draft_reports["duplicate"]["duplicate_events"] == 1
    ids = lambda name: {p["canonical"]:p["id"] for p in draft_reports[name]["proposals"]}
    assert "слот" not in ids("undo-last")
    assert ids("other-scope") == {"свитч":ids("other-scope")["свитч"]}
    assert all(p["scope"]=="technical" for p in draft_reports["accepted"]["proposals"])
    for name in ["accepted", "repeated", "undo-one"]: assert ids(name)["слот"] == ids("accepted")["слот"]
    evidence = lambda name: next(p["evidence_ids"] for p in draft_reports[name]["proposals"] if p["canonical"]=="слот")
    assert evidence("accepted")==["slot"] and evidence("repeated")==["repeat-slot","slot"] and evidence("undo-one")==["repeat-slot"]
    assert all(p["conflicting_proposals"] for p in draft_reports["conflict"]["proposals"] if p["alias"]=="свод")

    summary = []; breakdown = {}
    cohorts = {"draft-controls": "data/fixtures/feedback-drafts/corpus.jsonl"}
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
        for name in ["duplicate", "repeated", "undo-one"]:
            for filename in ["predictions.jsonl", "metrics.json"]:
                assert (directory / name / filename).read_bytes() == (directory / "accepted" / filename).read_bytes()
        references = {name: "accepted" for name in configs}
        references["disabled"] = "disabled"; references["accepted"] = "disabled"
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

    # Six representative old configurations: runtime correction code was not modified.
    historic = ["013-weighted/new-weighted/edit-1", "013-weighted/new-weighted/weighted-wide",
        "012-feedback/feedback-controls/source-only", "012-feedback/feedback-controls/joint-context",
        "011-term-families/new-families/flat-complete", "011-term-families/new-families/family"]
    regression = output / "regression"; regression.mkdir(); equal_profiles = 0
    for index, folder in enumerate(historic):
        old = root / "experiments" / folder
        manifest = json.loads((old / "manifest.json").read_text())
        command = manifest["arguments"].copy(); target = regression / f"{index:02}"
        assert sha(Path(manifest["input_file"])) == manifest["input_sha256"]
        assert sha(Path(command[command.index("--config")+1])) == manifest["resources"]["config_sha256"]
        command[command.index("--output-dir")+1] = str(target)
        run([binary, *command])
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (old / filename).read_bytes() == (target / filename).read_bytes()
        equal_profiles += 1
    assert equal_profiles == 6

    # Transactional validation and no-overwrite checks for the standalone CLI.
    invalid = scenarios()["accepted"][0].copy()
    conflict = copy.deepcopy(invalid[0]); conflict["after"] = "сняли свитч памяти"; invalid.append(conflict)
    invalid_path = output / "events" / "invalid-id.jsonl"
    with invalid_path.open("x") as stream:
        for event in invalid: stream.write(json.dumps(event,ensure_ascii=False)+"\n")
    bad_command = [str(draft_binary), str(invalid_path), "--config", str(reference_config), "--scope", "technical", "--output-dir", str(output / "invalid-output")]
    rejected = subprocess.run(bad_command,capture_output=True,text=True)
    assert rejected.returncode and not (output / "invalid-output").exists()
    save(output / "invalid-check.json", {"command":bad_command, "exit_code":rejected.returncode,"stderr":rejected.stderr})
    existing = output / "drafts" / "accepted"
    before = {f.name:sha(f) for f in existing.iterdir()}
    command = [str(draft_binary),str(output / "events/accepted.jsonl"),"--config",str(reference_config),"--scope","technical","--output-dir",str(existing)]
    rejected = subprocess.run(command,capture_output=True,text=True)
    assert rejected.returncode and before == {f.name:sha(f) for f in existing.iterdir()}
    save(output / "overwrite-check.json", {"command":command,"exit_code":rejected.returncode,"stderr":rejected.stderr,"unchanged_output_sha256":before})

    # Verify resources and compiled code for every run, including historical controls.
    baseline_path = Path(configs["accepted"]["baseline_config"])
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
    assert source_reports == 15
    draft_source_reports = draft_resource_reports = 0
    for path in (output / "drafts").glob("*/draft-manifest.json"):
        manifest = json.loads(path.read_text())
        assert sha(Path(manifest["input_file"])) == manifest["input_sha256"]
        assert sha(reference_config) == manifest["reference_config_sha256"]
        for filename,digest in manifest["output_sha256"].items(): assert sha(path.parent / filename)==digest
        for file,digest in manifest["compiled_source_sha256"].items(): assert sha(root / file)==digest
        draft_source_reports += 1
        for key,digest in manifest["resources"]["resource_sha256"].items():
            assert hashes[key]==digest; draft_resource_reports += 1
    assert draft_source_reports == 9 and draft_resource_reports == 45
    with (output / "comparison.csv").open("x", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0], lineterminator="\n"); writer.writeheader(); writer.writerows(summary)
    save(output / "summary.json", summary); save(output / "breakdown.json", breakdown); save(output / "commands.json", commands)
    save(output / "environment.json", {"implementation_commit": subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        "implementation_status": subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "configs", "data"], text=True).splitlines(),
        "platform": platform.platform(), "binary_sha256": sha(binary),
        "draft_binary_sha256":sha(draft_binary), "draft_source_reports":draft_source_reports,"draft_resource_reports":draft_resource_reports,
        "runner_sha256": {name: sha(root / "scripts" / name) for name in ["run_draft_experiment.py", "draft_profiles.py", "prepare_draft_fixture.py", "run_term_experiment.py"]},
        "corpus_sha256": {name: sha(root / folder) for name, folder in cohorts.items()},
        "source_fingerprint_reports": source_reports, "resource_hashes_verified": resource_reports, "legacy_byte_equal_profiles": equal_profiles,
        "reference_config_sha256": sha(reference_config),
        "scope": "offline disabled suggestions; manual acceptance simulated only in experiment; synthetic events/dev controls; six representative historical runs exactly reproduced, correction runtime unchanged"})


if __name__ == "__main__":
    main()
