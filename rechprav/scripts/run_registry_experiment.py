#!/usr/bin/env python3
"""015: replay frozen acceptance scenarios; verify lifecycle and independent span counts."""
import argparse
import copy
import csv
import json
import platform
import subprocess
import tomllib
from pathlib import Path

from registry_profiles import scenarios
from run_term_experiment import save, sha


def jsonl(path, rows):
    with path.open("x", encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    output.mkdir()
    for name in ["events", "decisions", "reviews", "registry", "benchmark"]:
        (output / name).mkdir()
    binary = root / "target/release/rechprav"
    registry = root / "target/release/examples/feedback_registry"
    reference = root / "configs/terms-families.toml"
    corpus = root / "data/fixtures/feedback-drafts/corpus.jsonl"
    old = root / "experiments/014-feedback-drafts-v2/draft-controls"
    snapshots, cases = scenarios()
    commands = []

    def run(command, success=True):
        command = list(map(str, command))
        commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if success and result.returncode:
            save(output / "failure.json", dict(command=command, stderr=result.stderr))
            result.check_returncode()
        if not success:
            assert result.returncode, command
        return result

    def command(events, decisions, scope, target, config=reference):
        return [registry, events, "--decisions", decisions, "--scope", scope,
                "--config", config, "--output-dir", target]

    for name, events in snapshots.items():
        jsonl(output / "events" / f"{name}.jsonl", events)
    empty = output / "decisions/empty.jsonl"
    jsonl(empty, [])
    reviews = {}
    # Only generate checkpoint views referenced by the pre-frozen recipes.
    for snapshot, scope in sorted({(s, scope) for _, recipes, _, _ in cases.values()
                                   for s, scope, _, _ in recipes}):
        target = output / "reviews" / f"{snapshot}-{scope}"
        run(command(output / "events" / f"{snapshot}.jsonl", empty, scope, target))
        report = json.loads((target / "report.json").read_text())
        assert not report["config"]["enabled"]
        tokens = {t["proposal_id"]: t for t in report["review_tokens"]}
        reviews[snapshot, scope] = [(p["canonical"], tokens[p["id"]]) for p in report["draft"]["proposals"]]

    reports = {}
    for name, (snapshot, recipes, scope, expected_active) in cases.items():
        decisions = []
        for s, selected_scope, canonical, action in recipes:
            selected = [t for c, t in reviews[s, selected_scope] if canonical == "*" or c == canonical]
            assert selected
            for token in selected:
                decisions.append(dict(schema_version=1, decision_id=f"decision-{len(decisions):03}", action=action, review=token))
        if name == "duplicate-decision":
            decisions += copy.deepcopy(decisions)
        path = output / "decisions" / f"{name}.jsonl"
        jsonl(path, decisions)
        target = output / "registry" / name
        run(command(output / "events" / f"{snapshot}.jsonl", path, scope, target))
        report = json.loads((target / "report.json").read_text())
        reports[name] = report
        config = tomllib.loads((target / "active.toml").read_text())
        assert config == report["config"]
        active = {t["id"] for s in config["sets"] for t in s["terms"]}
        assert len(active) == expected_active, name
        assert active == {s["proposal_id"] for s in report["states"] if s["status"] == "accepted"}
        assert config["enabled"] == bool(active)
        assert all(s["enabled"] == bool(s["terms"]) for s in config["sets"])
        assert all(t["enabled"] and t["boost"] == 0 for s in config["sets"] for t in s["terms"])
        print(name, "active:", len(active), flush=True)

    slot_id = next(p["id"] for p in reports["accepted"]["draft"]["proposals"] if p["canonical"] == "слот")
    state = lambda name: next(s for s in reports[name]["states"] if s["proposal_id"] == slot_id)
    for name in ["unapproved-repeat", "reborn-stale"]:
        assert state(name)["status"] == "stale_evidence"
    for name in ["undo-last", "new-context"]:
        assert state(name)["status"] == "no_active_evidence"
    assert state("approved-repeat")["surviving_evidence_ids"] == ["repeat-slot"]
    assert state("reborn-accepted")["surviving_evidence_ids"] == ["new-slot"]
    assert state("disabled")["status"] == "disabled"
    assert state("rejected")["status"] == "rejected"
    assert reports["duplicate-decision"]["duplicate_decisions"] == 8
    assert reports["accepted"]["config"] == tomllib.loads((root / "experiments/014-feedback-drafts-v2/configs/accepted.toml").read_text())

    # A reference config change must invalidate prior acceptance even with identical text.
    changed = output / "changed-reference.toml"
    baseline = Path(reports["accepted"]["config"]["baseline_config"])
    original_text = reference.read_text()
    original_baseline = tomllib.loads(original_text)["baseline_config"]
    changed.write_text(original_text.replace(original_baseline, str(baseline)) + "\n# environment change check\n")
    stale_target = output / "registry/stale-environment"
    run(command(output / "events/initial.jsonl", output / "decisions/accepted.jsonl", "technical", stale_target, changed))
    stale = json.loads((stale_target / "report.json").read_text())
    assert not stale["config"]["enabled"]
    assert all(s["status"] == "stale_environment" for s in stale["states"])
    reports["stale-environment"] = stale

    validation = json.loads(run([binary, "validate", corpus]).stdout)
    assert validation["records"] == 28 and validation["real_asr"] == validation["pending"] == validation["ambiguous"] == 0
    save(output / "validation.json", validation)
    records = [json.loads(row) for row in corpus.read_text().splitlines()]
    summary = []
    comparison = [[r["id"], r["source"], r["gold"]] for r in records]
    for name in reports:
        target = output / "benchmark" / name
        config = output / "registry" / name / "active.toml"
        run([binary, "terms-benchmark", corpus, "--config", config, "--output-dir", target])
        manifest = json.loads((target / "manifest.json").read_text())
        assert manifest["input_sha256"] == sha(corpus)
        assert manifest["resources"]["effective_config"] == reports[name]["config"]
        assert manifest["resources"]["config_sha256"] == sha(config)
        metrics = json.loads((target / "metrics.json").read_text())
        predictions = [json.loads(row) for row in (target / "predictions.jsonl").read_text().splitlines()]
        tp = fp = fn = harmed = gold_positions = 0
        errors = []
        for i, (record, prediction) in enumerate(zip(records, predictions, strict=True)):
            assert record["id"] == prediction["id"] and prediction["complete"]
            signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
            expected = {signature(e) for e in record["edits"]}
            proposed = {signature(e) for e in prediction["edits"]}
            tp += len(expected & proposed)
            fp += len(proposed - expected)
            fn += len(expected - proposed)
            mask = lambda edits: {t for e in edits for t in range(e["token_start"], e["token_end"])}
            harmed += len(mask(prediction["edits"]) - mask(record["edits"]))
            gold_positions += len(mask(record["edits"]))
            source = record["source"].encode()
            parts, cursor = [], 0
            for edit in prediction["edits"]:
                assert cursor <= edit["byte_start"] < edit["byte_end"] <= len(source)
                parts.extend([source[cursor:edit["byte_start"]], edit["replacement"].encode()])
                cursor = edit["byte_end"]
            parts.append(source[cursor:])
            assert b"".join(parts).decode() == prediction["prediction"]
            comparison[i].append(prediction["prediction"])
            if record["gold"] != prediction["prediction"]:
                errors.append(dict(id=record["id"], source=record["source"], gold=record["gold"], prediction=prediction["prediction"]))
        assert (tp, fp, fn, harmed) == tuple(metrics["span_counts"][k] for k in ["tp", "fp", "fn_", "harmed_correct_input"])
        assert metrics["span_counts"]["correct_input"] == metrics["source_tokens"] - gold_positions
        assert metrics["incomplete_records"] == 0
        save(target / "errors.json", errors)
        summary.append(dict(profile=name, active_rules=sum(len(s["terms"]) for s in reports[name]["config"]["sets"]),
                            tp=tp, fp=fp, fn=fn, harmed_correct_input=harmed,
                            exact_records=metrics["exact_records"], wer_after=metrics["wer_after"]))

    historical = {"accepted": "accepted", "approved-repeat": "accepted", "reborn-accepted": "accepted",
                  "duplicate-decision": "accepted", "reenabled": "accepted", "conflict-unapproved": "accepted",
                  "unapproved-repeat": "undo-last", "undo-last": "undo-last", "reborn-stale": "undo-last",
                  "disabled": "undo-last", "rejected": "undo-last", "new-context": "undo-last",
                  "conflict-accepted": "conflict", "other-accepted": "other-scope",
                  "unreviewed": "disabled", "other-unreviewed": "disabled", "stale-environment": "disabled"}
    for name, old_name in historical.items():
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (output / "benchmark" / name / filename).read_bytes() == (old / old_name / filename).read_bytes(), (name, filename)

    # Invalid input must fail before creating output; an existing snapshot stays untouched.
    invalid_checks = []
    rewritten = copy.deepcopy(snapshots["initial"])
    rewritten[0]["after"] = "сняли свитч памяти"
    jsonl(output / "events/rewritten.jsonl", rewritten)
    decisions = [json.loads(row) for row in (output / "decisions/accepted.jsonl").read_text().splitlines()]
    conflicting = copy.deepcopy(decisions[0])
    conflicting["action"] = "disable"
    jsonl(output / "decisions/conflicting.jsonl", decisions + [conflicting])
    for label, events, decisions in [("rewritten-prefix", "rewritten", "accepted"), ("conflicting-decision", "initial", "conflicting")]:
        target = output / label
        cmd = command(output / "events" / f"{events}.jsonl", output / "decisions" / f"{decisions}.jsonl", "technical", target)
        result = run(cmd, success=False)
        assert not target.exists()
        invalid_checks.append(dict(case=label, stderr=result.stderr, exit_code=result.returncode))
    existing = output / "registry/accepted"
    before = {p.name: sha(p) for p in existing.iterdir()}
    result = run(command(output / "events/initial.jsonl", output / "decisions/accepted.jsonl", "technical", existing), success=False)
    assert before == {p.name: sha(p) for p in existing.iterdir()}
    invalid_checks.append(dict(case="no-overwrite", stderr=result.stderr, exit_code=result.returncode, unchanged_sha256=before))
    save(output / "invalid-checks.json", invalid_checks)

    # Check all manifests against files used by this run, not only a reported version string.
    base_config = tomllib.loads(baseline.read_text())
    paths = {"config": baseline, **{key: baseline.parent / base_config[key] for key in ["dictionary", "domain", "ignore"]},
             "frequency": baseline.parent / base_config["frequency"]["path"]}
    resource_hashes = {key: sha(path) for key, path in paths.items()}
    source_count = resource_count = registry_count = 0
    for path in output.rglob("*manifest.json"):
        m = json.loads(path.read_text())
        for filename, digest in m["compiled_source_sha256"].items():
            assert sha(root / filename) == digest, (path, filename)
        source_count += 1
        assert sha(Path(m["input_file"])) == m["input_sha256"]
        resources = m["resources"]
        if path.name == "registry-manifest.json":
            registry_count += 1
            assert sha(Path(m["decisions_file"])) == m["decisions_sha256"]
            expected_reference = changed if path.parent == stale_target else reference
            assert sha(expected_reference) == m["reference_config_sha256"]
            for filename, digest in m["output_sha256"].items():
                assert sha(path.parent / filename) == digest
        elif "baseline_resources" in resources:
            resources = resources["baseline_resources"]
        if resources:
            for key, digest in resources["resource_sha256"].items():
                assert digest == resource_hashes[key]
                resource_count += 1
    assert registry_count == len(reviews) + len(reports)
    assert source_count == registry_count + len(reports)
    for filename, rows, fields in [("comparison.csv", summary, list(summary[0])),
                                   ("case-comparison.csv", comparison, ["id", "source", "gold", *reports])]:
        with (output / filename).open("x", newline="", encoding="utf-8") as stream:
            writer = csv.writer(stream, lineterminator="\n")
            writer.writerow(fields)
            writer.writerows([[r[k] for k in fields] if isinstance(r, dict) else r for r in rows])
    save(output / "summary.json", summary)
    save(output / "commands.json", commands)
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "configs", "data"], text=True).splitlines(),
        platform=platform.platform(), binary_sha256=sha(binary), registry_binary_sha256=sha(registry),
        corpus_sha256=sha(corpus), reference_config_sha256=sha(reference),
        runner_sha256={n: sha(root / "scripts" / n) for n in ["run_registry_experiment.py", "registry_profiles.py", "prepare_draft_fixture.py", "run_term_experiment.py"]},
        verified_source_manifests=source_count, verified_resource_hashes=resource_count,
        registry_manifests=registry_count, historical_byte_equal_profiles=len(historical),
        scope="offline explicit acceptance lifecycle; existing 28 synthetic/dev examples; correction runtime unchanged"))
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
