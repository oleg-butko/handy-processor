#!/usr/bin/env python3
"""020: frozen generation policies, fresh review, legacy replay and publication/reader."""
import argparse
import copy
import csv
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
from mixed_profiles import EVENTS, bindings, corpus, scenarios
from run_term_experiment import save, sha, write_config


def jsonl(path, rows):
    with path.open("x") as stream:
        for row in rows: stream.write(json.dumps(row, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve(); output.mkdir()
    root = Path(__file__).resolve().parents[1]
    previous = root / "experiments/017-profile-composition-v3"
    for name in ["inputs", "reviews", "composed", "configs", "benchmark", "legacy", "publication"]: (output / name).mkdir()
    binary = root / "target/release/rechprav"
    composer = root / "target/release/examples/feedback_compose"
    commands = []

    def run(command, success=True, stdin=None, log=None):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, input=stdin, capture_output=True, text=True)
        if log:
            with (output / log).open("x") as stream: stream.write((result.stdout + result.stderr).rstrip() + "\n")
        if (result.returncode == 0) != success:
            save(output / "failure.json", dict(command=command, stdout=result.stdout, stderr=result.stderr));
            raise AssertionError(result.stderr)
        return result

    def compose(target, config, routes, events, decisions):
        run([composer, events, "--config", config, "--bindings", routes, "--decisions", decisions,
             "--scope", "technical", "--output-dir", target])
        return json.loads((target / "report.json").read_text())

    cases = corpus(); corpus_path = output / "inputs/corpus.jsonl"; jsonl(corpus_path, cases)
    initial = output / "inputs/initial.jsonl"; jsonl(initial, EVENTS)
    empty = output / "inputs/empty.jsonl"; empty.write_text("")
    empty_routes = output / "inputs/empty-bindings.json"; save(empty_routes, dict(enabled=True, bindings=[]))
    profiles = scenarios()
    baseline = root / "configs/opencorpora-packed-frequency-b2.toml"
    for name, (config, _, events) in profiles.items():
        config["baseline_config"] = str(baseline)
        write_config(output / "inputs" / f"{name}.toml", config)
        jsonl(output / "inputs" / f"{name}.jsonl", events)
    seed = compose(output / "reviews/seed", output / "inputs/exact.toml", empty_routes, initial, empty)
    proposals = seed["registry"]["draft"]["proposals"]; assert len(proposals) == 5
    reports = {}; configs = {}
    for name, (_, settings, _) in profiles.items():
        manual = output / "inputs" / f"{name}.toml"
        routes = output / "inputs" / f"{name}-bindings.json"
        settings["bindings"] = bindings(proposals, name); save(routes, settings)
        review = compose(output / "reviews" / name, manual, routes, initial, empty)
        decisions = output / "inputs" / f"{name}-decisions.jsonl"
        jsonl(decisions, [dict(schema_version=1, decision_id=f"accept-{i}", action="accept", review=t)
                         for i, t in enumerate(review["registry"]["review_tokens"])])
        report = compose(output / "composed" / name, manual, routes, output / "inputs" / f"{name}.jsonl", decisions)
        reports[name] = report
        before, after = review["config"], report["config"]
        assert before["policy"] == after["policy"] and before["enabled"] == after["enabled"]
        assert before.get("source_penalties", []) == after.get("source_penalties", [])
        for old, new in zip(before["sets"], after["sets"]):
            assert {k:v for k,v in old.items() if k != "terms"} == {k:v for k,v in new.items() if k != "terms"}
            assert new["terms"][:len(old["terms"])] == old["terms"]
        ids = {a["proposal_id"] for a in report["audit"] if a["status"] == "added"}
        generated = [t for s in after["sets"] for t in s["terms"] if t["id"] in ids]
        assert all(t["generation"] == "exact" for t in generated)
        config = copy.deepcopy(after)
        if name == "unpinned":
            for group in config["sets"]:
                for term in group["terms"]:
                    if term["id"] in ids: term.pop("generation")
        path = output / "configs" / f"{name}.toml"; write_config(path, config); configs[name] = path
        print(f"composed {name}: {len(generated)} pinned rules", flush=True)

    # Isolate source fingerprint drift: use EXACTLY the old events/config/routes/decisions.
    old_input = previous / "inputs"
    stale = compose(output / "composed/stale-runtime", old_input / "composed.toml", old_input / "composed-bindings.json",
                    old_input / "composed.jsonl", old_input / "composed-decisions.jsonl")
    assert all(s["status"] == "stale_environment" for s in stale["registry"]["states"])
    assert not any(a["status"] == "added" for a in stale["audit"])
    save(output / "stale-runtime.json", dict(states=stale["registry"]["states"], added_rules=0))
    validation = json.loads(run([binary, "validate", corpus_path]).stdout)
    assert validation["records"] == 30 and validation["real_asr"] == validation["pending"] == 0
    save(output / "corpus-validation.json", validation)
    results = {}; summary = []
    for name, path in configs.items():
        bench = output / "benchmark" / name
        run([binary, "terms-benchmark", corpus_path, "--config", path, "--output-dir", bench])
        predictions = [json.loads(s) for s in (bench / "predictions.jsonl").read_text().splitlines()]
        results[name] = predictions
        metrics = json.loads((bench / "metrics.json").read_text())
        counts = metrics["span_counts"]
        # Independent exact-span check, not a token zip.
        tp = fp = fn = 0
        for row, prediction in zip(cases, predictions, strict=True):
            assert row["id"] == prediction["id"] and prediction["complete"]
            sig = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
            gold = {sig(e) for e in row["edits"]}; proposed = {sig(e) for e in prediction["edits"]}
            tp += len(gold & proposed); fp += len(proposed-gold); fn += len(gold-proposed)
        assert (tp, fp, fn) == (counts["tp"], counts["fp"], counts["fn_"])
        summary.append(dict(profile=name, tp=tp, fp=fp, fn=fn, exact_records=metrics["exact_records"],
                            added_rules=sum(a["status"] == "added" for a in reports[name]["audit"]),
                            publishable=name != "unpinned"))
        print(summary[-1], flush=True)
    def prediction(name, source):
        return next(p for p in results[name] if p["source"] == source)
    for name in ["mixed", "weighted"]:
        assert prediction(name, "долгождать")["prediction"] == "долго ждать"
        assert prediction(name, "открой калиборт")["prediction"] == "открой клипборд"
        for source, target in [("включи катэхи", "КТХ"), ("проверь пээнзэ сегодня", "ПНЗ"), ("проверь пэ энзэ сегодня", "ПНЗ")]:
            assert not any(c["canonical"] == target for d in prediction(name, source)["decisions"] for c in d["candidates"])
    assert results["mixed"] == results["duplicate"]
    assert prediction("conflict", "сняли свод памяти")["prediction"] == "сняли свод памяти"
    assert prediction("owner-high-conflict", "сняли свод памяти")["prediction"] == "сняли слот памяти"
    assert prediction("excluded", "сняли свод памяти")["prediction"] == "сняли свод памяти"
    assert all(p["source"] == p["prediction"] for p in results["global-off"])

    # Legacy configs have no generation; verify complete predictions, metrics and traces.
    legacy = []
    for old_bench in sorted((previous / "benchmark").iterdir()):
        manifest = json.loads((old_bench / "manifest.json").read_text())
        config = previous / "composed" / old_bench.name / "composed.toml"
        target = output / "legacy" / f"017-{old_bench.name}"
        run([binary, "terms-benchmark", manifest["input_file"], "--config", config, "--output-dir", target])
        for filename in ["predictions.jsonl", "metrics.json"]: assert (target / filename).read_bytes() == (old_bench / filename).read_bytes()
        legacy.append(dict(profile=f"017-{old_bench.name}", equal=True, predictions_sha256=sha(target / "predictions.jsonl")))
    weighted = root / "experiments/013-weighted"
    for old_bench in sorted((weighted / "new-weighted").iterdir()):
        if not (old_bench / "manifest.json").exists(): continue
        m = json.loads((old_bench / "manifest.json").read_text()); target = output / "legacy" / f"013-{old_bench.name}"
        run([binary, "terms-benchmark", m["input_file"], "--config", weighted / "configs" / f"{old_bench.name}.toml", "--output-dir", target])
        for filename in ["predictions.jsonl", "metrics.json"]: assert (target / filename).read_bytes() == (old_bench / filename).read_bytes()
        legacy.append(dict(profile=f"013-{old_bench.name}", equal=True, predictions_sha256=sha(target / "predictions.jsonl")))
    save(output / "legacy-equality.json", legacy)
    save(output / "summary.json", summary)
    with (output / "comparison.csv").open("x", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0]); writer.writeheader(); writer.writerows(summary)
    with (output / "case-comparison.csv").open("x", newline="") as stream:
        writer = csv.writer(stream); writer.writerow(["id", "source", "gold", *results])
        for i, row in enumerate(cases): writer.writerow([row["id"], row["source"], row["gold"], *[p[i]["prediction"] for p in results.values()]])

    # Publication and reader use freshly accepted exports; old exports remain immutable.
    cli = root / "publication/cli.mjs"; pub = output / "publication"
    database = Path(tempfile.mkdtemp(prefix="publication-020-", dir=root / "artifacts")) / "profiles.sqlite"
    run(["node", cli, "create", database, "technical"])
    snaps = []
    for revision, name in enumerate(["mixed", "global-off"]):
        bundle_path = pub / f"{name}-bundle.json"
        run(["node", cli, "pack", output / "composed" / name, output / "inputs" / f"{name}.toml", bundle_path])
        bundle = json.loads(bundle_path.read_text())
        request = pub / f"{name}-request.json"
        save(request, dict(schema_version=1, request_id=name, expected_revision=revision, bundle=bundle))
        run(["node", cli, "request", database, request]); run(["node", cli, "publish", database, name])
        snap = json.loads(run(["node", cli, "snapshot", database]).stdout); save(pub / f"{name}-snapshot.json", snap); snaps.append(snap)
    driver = [dict(kind="reload", snapshot=snaps[0]), dict(kind="acquire", id="old"),
              dict(kind="reload", snapshot=snaps[1]), dict(kind="acquire", id="new")]
    for row in cases:
        for handle in ["old", "new"]: driver.append(dict(kind="analyze", id=handle, text=row["source"]))
    jsonl(pub / "reader-input.jsonl", driver)
    responses = run([root / "target/release/examples/profile_reader", "technical"], stdin=(pub / "reader-input.jsonl").read_text()).stdout
    with (pub / "reader-output.jsonl").open("x") as stream: stream.write(responses)
    responses = [json.loads(s) for s in responses.splitlines()]; assert all(r["result"]["ok"] for r in responses)
    for i, response in enumerate(responses[4:]):
        out = response["result"]["output"]; expected = results["mixed" if i % 2 == 0 else "global-off"][i // 2]
        assert out["generation"] == (1 if i % 2 == 0 else 2)
        assert out["analysis"]["corrected"] == expected["prediction"]
        for field in ["edits", "decisions", "complete", "comparisons"]: assert out["analysis"][field] == expected[field]
    old = json.loads((root / "experiments/018-profile-publication/snapshots/initial.json").read_text())
    rejected = run([root / "target/release/examples/profile_validate"], success=False, stdin=json.dumps(old["active"]["bundle"]))
    assert "producer compiled sources mismatch" in rejected.stderr
    save(pub / "old-export-rejected.json", dict(stderr=rejected.stderr))
    run(["/home/user1/.local/bin/cargo", "test", "--manifest-path", root / "Cargo.toml", "--locked", "--offline"], log="rust-tests.log")
    run(["node", "--test", "--test-reporter=tap", root / "publication/store.test.mjs"], log="publication-tests.tap")
    manifests = 0
    for path in output.rglob("*manifest.json"):
        m = json.loads(path.read_text()); manifests += 1
        for filename, digest in m["compiled_source_sha256"].items(): assert sha(root / filename) == digest
        assert sha(Path(m["input_file"])) == m["input_sha256"]
        if path.name == "composition-manifest.json":
            for filename, digest in m["output_sha256"].items(): assert sha(path.parent / filename) == digest
            assert sha(Path(m["bindings_file"])) == m["bindings_sha256"]
            assert sha(Path(m["decisions_file"])) == m["decisions_sha256"]
    save(output / "commands.json", commands)
    files = ["scripts/mixed_profiles.py", "scripts/run_mixed_experiment.py", "scripts/composition_profiles.py", "scripts/run_term_experiment.py",
             "src/terms.rs", "src/feedback_composition.rs", "tests/generation.rs", "tests/feedback_composition.rs",
             "publication/store.test.mjs", "publication/testing/fixtures.mjs"]
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", root, "status", "--porcelain", "--", "src", "examples", "scripts", "tests", "publication"], text=True).splitlines(),
        source_sha256={name:sha(root / name) for name in files}, corpus_sha256=sha(corpus_path), manifests_verified=manifests,
        binary_sha256=sha(binary), previous_binary_sha256=sha(root / "artifacts/020-mixed-generation-baseline/rechprav"),
        composer_binary_sha256=sha(composer), validator_binary_sha256=sha(root / "target/release/examples/profile_validate"),
        reader_binary_sha256=sha(root / "target/release/examples/profile_reader"),
        database_path=str(database), legacy_profiles=len(legacy), reader_comparisons=60,
        claim="fixed synthetic/dev generation controls; no real ASR quality estimate"))
    print(f"done: {len(summary)} profiles, {len(legacy)} legacy profiles unchanged, 60 reader comparisons", flush=True)


if __name__ == "__main__": main()
