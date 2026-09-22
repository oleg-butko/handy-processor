#!/usr/bin/env python3
"""016: actual durable capture CLI -> Rust review/acceptance -> correction benchmark."""
import argparse
import csv
import json
import platform
import subprocess
import tempfile
import tomllib
from pathlib import Path

from capture_profiles import fixture
from run_term_experiment import save, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    output.mkdir()
    for folder in ["requests", "capture", "registry", "decisions", "benchmark"]:
        (output / folder).mkdir()
    binary = root / "target/release/rechprav"
    registry = root / "target/release/examples/feedback_registry"
    cli = root / "editor/capture-cli.mjs"
    reference = root / "configs/terms-families.toml"
    corpus = root / "data/fixtures/feedback-drafts/corpus.jsonl"
    source_paths = ["editor/capture-store.mjs", "editor/capture-cli.mjs", "editor/capture-store.test.mjs",
                    "editor/testing/process-worker.mjs", "scripts/capture_profiles.py", "scripts/run_capture_experiment.py",
                    "scripts/prepare_draft_fixture.py", "scripts/run_term_experiment.py"]
    source_hashes = {name: sha(root / name) for name in source_paths}
    database = Path(tempfile.mkdtemp(prefix="016-capture-", dir=root / "artifacts")) / "capture.sqlite3"
    commands = []

    def run(command):
        command = list(map(str, command))
        commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode:
            save(output / "failure.json", dict(command=command, stderr=result.stderr, stdout=result.stdout))
            result.check_returncode()
        return result.stdout

    tests = run(["node", "--test", "--test-reporter=tap", root / "editor/capture-store.test.mjs"])
    (output / "capture-tests.tap").write_text(tests)
    assert "# tests 16" in tests and "# fail 0" in tests
    run(["node", cli, "create", database])
    data = fixture()
    save(output / "frozen-inputs.json", data)
    receipts = []

    def append(request):
        path = output / "requests" / f"{request['request_id']}.json"
        save(path, request)
        result = json.loads(run(["node", cli, "append", database, path]))
        receipts.append(result)
        assert not result["replayed"]

    def export(name, expected):
        target = output / "capture" / name
        run(["node", cli, "export", database, target])
        events = [json.loads(line) for line in (target / "events.jsonl").read_text().splitlines()]
        assert events == expected, name
        snapshot = json.loads((target / "snapshot.json").read_text())
        assert snapshot["events"] == expected
        assert [r["sequence"] for r in snapshot["receipts"]] == list(range(1, len(expected) + 1))
        return target

    for request in data["initial"]:
        append(request)
    export("initial", data["expected_initial"])
    duplicate = json.loads(run(["node", cli, "append", database, output / "requests/slot.json"]))
    assert duplicate == {**receipts[0], "replayed": True}
    save(output / "duplicate-receipt.json", duplicate)
    export("duplicate", data["expected_initial"])
    for name in ["events.jsonl", "requests.jsonl", "snapshot.json", "capture-manifest.json"]:
        assert (output / "capture/initial" / name).read_bytes() == (output / "capture/duplicate" / name).read_bytes()
    for request in data["undo"]:
        append(request)
    export("undo", data["expected_undo"])
    append(data["redo"])
    export("redo", data["expected_redo"])
    save(output / "receipts.json", receipts)

    def jsonl(path, rows):
        with path.open("x") as stream:
            for row in rows:
                stream.write(json.dumps(row, ensure_ascii=False) + "\n")

    empty = output / "decisions/empty.jsonl"
    jsonl(empty, [])

    def rebuild(name, snapshot, decisions, scope="technical"):
        target = output / "registry" / name
        run([registry, output / "capture" / snapshot / "events.jsonl", "--decisions", decisions,
             "--config", reference, "--scope", scope, "--output-dir", target])
        return json.loads((target / "report.json").read_text())

    review = rebuild("review-initial", "initial", empty)
    assert len(review["draft"]["proposals"]) == 4
    assert not review["config"]["enabled"]
    audits = {r["event_id"]: r["status"] for r in review["draft"]["audit"]}
    assert audits["style"] == "not_confirmed_asr" and audits["other"] == "other_scope"
    proposal = next(p for p in review["draft"]["proposals"] if p["canonical"] == "слот")
    assert proposal["evidence_ids"] == ["emoji", "slot"]
    decisions = [dict(schema_version=1, decision_id=f"accept-{i}", action="accept", review=token)
                 for i, token in enumerate(review["review_tokens"])]
    path = output / "decisions/accepted.jsonl"
    jsonl(path, decisions)
    profiles = {"accepted": rebuild("accepted", "initial", path),
                "undo": rebuild("undo", "undo", path),
                "redo-stale": rebuild("redo-stale", "redo", path)}
    row = lambda name: next(s for s in profiles[name]["states"] if s["proposal_id"] == proposal["id"])
    assert row("undo")["status"] == "no_active_evidence"
    assert row("redo-stale")["status"] == "stale_evidence"
    assert row("redo-stale")["current_evidence_ids"] == ["redo-slot"]
    redo_review = rebuild("review-redo", "redo", empty)
    token = next(t for t in redo_review["review_tokens"] if t["proposal_id"] == proposal["id"])
    jsonl(output / "decisions/redo.jsonl", decisions + [dict(schema_version=1, decision_id="accept-redo", action="accept", review=token)])
    profiles["redo-accepted"] = rebuild("redo-accepted", "redo", output / "decisions/redo.jsonl")
    assert row("redo-accepted")["surviving_evidence_ids"] == ["redo-slot"]
    other = rebuild("other-unreviewed", "initial", path, "other")
    assert not other["config"]["enabled"]
    assert len(other["draft"]["proposals"]) == 1 and other["draft"]["proposals"][0]["canonical"] == "свитч"

    # Independently select the same previously-tested 014 rules; adapter cannot widen them.
    old_config = tomllib.loads((root / "experiments/014-feedback-drafts-v2/configs/accepted.toml").read_text())
    for name, report in profiles.items():
        selected = {"клипборд", "долго ждать", "ПНЗ"}
        if name in ["accepted", "redo-accepted"]:
            selected.add("слот")
        expected_config = json.loads(json.dumps(old_config))
        expected_config["sets"][0]["terms"] = [t for t in old_config["sets"][0]["terms"] if t["canonical"] in selected]
        assert report["config"] == expected_config, name

    validation = json.loads(run([binary, "validate", corpus]))
    assert validation["records"] == 28 and validation["real_asr"] == validation["pending"] == 0
    save(output / "validation.json", validation)
    records = [json.loads(row) for row in corpus.read_text().splitlines()]
    comparison = [[r["id"], r["source"], r["gold"]] for r in records]
    summary = []
    for name, report in profiles.items():
        directory = output / "benchmark" / name
        config_path = output / "registry" / name / "active.toml"
        run([binary, "terms-benchmark", corpus, "--config", config_path, "--output-dir", directory])
        manifest = json.loads((directory / "manifest.json").read_text())
        assert manifest["resources"]["effective_config"] == report["config"]
        assert manifest["resources"]["config_sha256"] == sha(config_path)
        metrics = json.loads((directory / "metrics.json").read_text())
        predictions = [json.loads(row) for row in (directory / "predictions.jsonl").read_text().splitlines()]
        tp = fp = fn = harmed = 0
        errors = []
        for i, (record, prediction) in enumerate(zip(records, predictions, strict=True)):
            assert record["id"] == prediction["id"] and prediction["complete"]
            signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
            expected = {signature(e) for e in record["edits"]}
            proposed = {signature(e) for e in prediction["edits"]}
            tp += len(expected & proposed); fp += len(proposed - expected); fn += len(expected - proposed)
            mask = lambda edits: {t for e in edits for t in range(e["token_start"], e["token_end"])}
            harmed += len(mask(prediction["edits"]) - mask(record["edits"]))
            source = record["source"].encode()
            parts, cursor = [], 0
            for edit in prediction["edits"]:
                assert cursor <= edit["byte_start"] < edit["byte_end"] <= len(source)
                parts.extend([source[cursor:edit["byte_start"]], edit["replacement"].encode()]); cursor = edit["byte_end"]
            parts.append(source[cursor:])
            assert b"".join(parts).decode() == prediction["prediction"]
            comparison[i].append(prediction["prediction"])
            if record["gold"] != prediction["prediction"]:
                errors.append(dict(id=record["id"], source=record["source"], gold=record["gold"], prediction=prediction["prediction"]))
        assert (tp, fp, fn, harmed) == tuple(metrics["span_counts"][k] for k in ["tp", "fp", "fn_", "harmed_correct_input"])
        assert metrics["incomplete_records"] == 0
        save(directory / "errors.json", errors)
        summary.append(dict(profile=name, active_rules=sum(len(s["terms"]) for s in report["config"]["sets"]),
                            tp=tp, fp=fp, fn=fn, harmed_correct_input=harmed, exact_records=metrics["exact_records"], wer_after=metrics["wer_after"]))
    for a, b in [("accepted", "redo-accepted"), ("undo", "redo-stale")]:
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (output / "benchmark" / a / filename).read_bytes() == (output / "benchmark" / b / filename).read_bytes()

    baseline = Path(profiles["accepted"]["config"]["baseline_config"])
    config = tomllib.loads(baseline.read_text())
    paths = {"config": baseline, **{key: baseline.parent / config[key] for key in ["dictionary", "domain", "ignore"]},
             "frequency": baseline.parent / config["frequency"]["path"]}
    hashes = {key: sha(path) for key, path in paths.items()}
    source_count = resource_count = capture_count = 0
    for path in output.rglob("*manifest.json"):
        m = json.loads(path.read_text())
        if path.name == "capture-manifest.json":
            capture_count += 1
        else:
            source_count += 1
            for filename, digest in m["compiled_source_sha256"].items():
                assert sha(root / filename) == digest
            assert sha(Path(m["input_file"])) == m["input_sha256"]
            resources = m["resources"]
            if "baseline_resources" in resources:
                resources = resources["baseline_resources"]
            for key, digest in resources["resource_sha256"].items():
                assert hashes[key] == digest
                resource_count += 1
            if path.name == "registry-manifest.json":
                assert sha(Path(m["decisions_file"])) == m["decisions_sha256"]
                assert sha(reference) == m["reference_config_sha256"]
        for filename, digest in m.get("output_sha256", {}).items():
            assert sha(path.parent / filename) == digest
    assert source_count == 11 and resource_count == 55 and capture_count == 4
    assert source_hashes == {name: sha(root / name) for name in source_paths}
    with (output / "case-comparison.csv").open("x", newline="", encoding="utf-8") as stream:
        writer = csv.writer(stream, lineterminator="\n")
        writer.writerow(["id", "source", "gold", *profiles]); writer.writerows(comparison)
    save(output / "summary.json", summary)
    save(output / "commands.json", commands)
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", str(root), "status", "--porcelain", "--", "src", "examples", "editor", "scripts", "tests", "configs", "data"], text=True).splitlines(),
        node_version=subprocess.check_output(["node", "--version"], text=True).strip(), platform=platform.platform(),
        binary_sha256=sha(binary), registry_binary_sha256=sha(registry), corpus_sha256=sha(corpus),
        source_sha256=source_hashes, capture_tests=16, verified_source_manifests=source_count,
        verified_resource_hashes=resource_count, verified_capture_manifests=capture_count,
        generated_database=str(database), scope="offline capture of synthetic explicit fragment corrections; actual UI capture and live profile installation absent"))
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
