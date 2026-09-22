#!/usr/bin/env python3
"""017: frozen manual/generated composition ablations with fresh reviews and provenance."""
import argparse
import copy
import csv
import json
import subprocess
import tomllib
from pathlib import Path
from composition_profiles import EVENTS, bindings, corpus, scenarios
from run_term_experiment import save, sha, write_config


def jsonl(path, rows):
    with path.open("x") as stream:
        for row in rows: stream.write(json.dumps(row, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve()
    root = Path(__file__).resolve().parents[1]
    output.mkdir()
    for name in ["inputs", "reviews", "composed", "benchmark"]: (output / name).mkdir()
    binary = root / "target/release/rechprav"
    composer = root / "target/release/examples/feedback_compose"
    commands = []

    def run(command, success=True):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if success and result.returncode:
            save(output / "failure.json", dict(command=command, stderr=result.stderr)); result.check_returncode()
        if not success: assert result.returncode
        return result

    def compose(name, config, routes, events, decisions, review=False, success=True):
        target = output / ("reviews" if review else "composed") / name
        result = run([composer, events, "--config", config, "--bindings", routes, "--decisions", decisions,
                      "--scope", "technical", "--output-dir", target], success)
        if not success:
            assert not target.exists()
            return dict(stderr=result.stderr, exit_code=result.returncode)
        return json.loads((target / "report.json").read_text())

    records = corpus(); corpus_path = output / "inputs/corpus.jsonl"; jsonl(corpus_path, records)
    initial = output / "inputs/initial.jsonl"; jsonl(initial, EVENTS)
    empty = output / "inputs/empty.jsonl"; jsonl(empty, [])
    empty_routes = output / "inputs/empty-bindings.json"; save(empty_routes, dict(enabled=True, bindings=[]))
    cases = scenarios()
    baseline = (root / "configs/opencorpora-packed-frequency-b2.toml").resolve()
    for name, (config, settings, events) in cases.items():
        config["baseline_config"] = str(baseline)
        write_config(output / "inputs" / f"{name}.toml", config)
        jsonl(output / "inputs" / f"{name}.jsonl", events)
    seed = compose("seed", output / "inputs/composed.toml", empty_routes, initial, empty, True)
    proposals = seed["registry"]["draft"]["proposals"]
    assert len(proposals) == 4
    ids = {p["canonical"]:p["id"] for p in proposals}
    reports = {}; reviews = {}
    for name, (_, settings, _) in cases.items():
        config = output / "inputs" / f"{name}.toml"
        routes = output / "inputs" / f"{name}-bindings.json"
        settings["bindings"] = bindings(proposals, name); save(routes, settings)
        review = compose(name, config, routes, initial, empty, True); reviews[name] = review
        decisions = [dict(schema_version=1, decision_id=f"accept-{i}", action="accept", review=t)
                     for i,t in enumerate(review["registry"]["review_tokens"])]
        decision_path = output / "inputs" / f"{name}-decisions.jsonl"; jsonl(decision_path, decisions)
        result = compose(name, config, routes, output / "inputs" / f"{name}.jsonl", decision_path)
        reports[name] = result
        before, after = review["config"], result["config"]
        assert after["enabled"] == before["enabled"] and after["policy"] == before["policy"]
        assert after.get("source_penalties", []) == before.get("source_penalties", [])
        for old_set, new_set in zip(before["sets"], after["sets"]):
            assert {k:v for k,v in old_set.items() if k != "terms"} == {k:v for k,v in new_set.items() if k != "terms"}
            assert new_set["terms"][:len(old_set["terms"])] == old_set["terms"]
        assert tomllib.loads((output / "composed" / name / "composed.toml").read_text()) == after
        print(name, {r["proposal_id"]:r["status"] for r in result["audit"]}, flush=True)

    status = lambda name, canonical: next(r["status"] for r in reports[name]["audit"] if r["proposal_id"] == ids[canonical])
    for name, expected in {"composed":"added", "manual-only":"unbound", "feedback-off":"feedback_disabled", "binding-off":"binding_disabled",
                           "owner-off":"owner_term_disabled", "set-off":"owner_set_disabled", "global-off":"manual_disabled",
                           "owner-deleted":"owner_missing", "new-over-existing":"existing_target_requires_owner", "unsupported-edit":"unsupported_mode",
                           "unsupported-cues":"unsupported_context_cues", "disjoint-context":"disjoint_context", "undo":"not_accepted"}.items():
        assert status(name, "слот") == expected, (name, status(name,"слот"))
    composed = reports["composed"]["config"]
    learned_form = next(t for s in composed["sets"] for t in s["terms"] if t["id"] == ids["слота"])
    assert learned_form["context_patterns"] == ["без снятия {term}"]
    assert learned_form.get("forms", []) == [] and "pronunciation" not in learned_form
    assert reports["duplicate"]["config"] == composed

    # Old decisions do not authorize changed weights/routes. Fresh-review cases above separately test guards.
    stale = compose("stale-settings", output / "inputs/composed.toml", output / "inputs/binding-off-bindings.json",
                    initial, output / "inputs/composed-decisions.jsonl")
    assert all(s["status"] == "stale_environment" for s in stale["registry"]["states"])
    assert stale["config"] == reviews["composed"]["config"]
    reports["stale-settings"] = stale
    bad = copy.deepcopy(cases["composed"][1]); bad["bindings"].append(copy.deepcopy(bad["bindings"][0]))
    bad_path = output / "inputs/duplicate-bindings.json"; save(bad_path, bad)
    save(output / "invalid-check.json", compose("invalid", output / "inputs/composed.toml", bad_path, initial, empty, success=False))

    validation = json.loads(run([binary,"validate",corpus_path]).stdout)
    assert validation["records"] == 20 and validation["real_asr"] == validation["pending"] == validation["ambiguous"] == 0
    save(output / "validation.json", validation)
    summary = []; comparison = [[r["id"],r["source"],r["gold"]] for r in records]
    predictions_by_name = {}
    for name, report in reports.items():
        directory = output / "benchmark" / name
        config = output / "composed" / name / "composed.toml"
        run([binary,"terms-benchmark",corpus_path,"--config",config,"--output-dir",directory])
        manifest = json.loads((directory/"manifest.json").read_text())
        assert manifest["resources"]["effective_config"] == report["config"]
        assert manifest["resources"]["config_sha256"] == sha(config)
        metrics = json.loads((directory/"metrics.json").read_text())
        predictions = [json.loads(line) for line in (directory/"predictions.jsonl").read_text().splitlines()]
        predictions_by_name[name] = predictions
        tp = fp = fn = harmed = 0; errors = []
        for i,(record,prediction) in enumerate(zip(records,predictions,strict=True)):
            assert record["id"] == prediction["id"] and prediction["complete"]
            signature = lambda e:(e["byte_start"],e["byte_end"],e["replacement"])
            expected = {signature(e) for e in record["edits"]}; proposed = {signature(e) for e in prediction["edits"]}
            tp += len(expected & proposed); fp += len(proposed-expected); fn += len(expected-proposed)
            mask = lambda edits:{t for e in edits for t in range(e["token_start"],e["token_end"])}
            harmed += len(mask(prediction["edits"])-mask(record["edits"]))
            source=record["source"].encode(); cursor=0; parts=[]
            for e in prediction["edits"]:
                assert cursor <= e["byte_start"] < e["byte_end"] <= len(source)
                parts.extend([source[cursor:e["byte_start"]],e["replacement"].encode()]); cursor=e["byte_end"]
            parts.append(source[cursor:]); assert b"".join(parts).decode() == prediction["prediction"]
            comparison[i].append(prediction["prediction"])
            if record["gold"] != prediction["prediction"]: errors.append(dict(id=record["id"], source=record["source"], gold=record["gold"], prediction=prediction["prediction"]))
        assert (tp,fp,fn,harmed) == tuple(metrics["span_counts"][k] for k in ["tp","fp","fn_","harmed_correct_input"])
        assert metrics["incomplete_records"] == 0
        save(directory/"errors.json", errors)
        summary.append(dict(profile=name, added_rules=sum(r["status"] == "added" for r in report["audit"]), tp=tp,fp=fp,fn=fn,
                            harmed_correct_input=harmed,exact_records=metrics["exact_records"],wer_after=metrics["wer_after"]))
    def prediction(name, source):
        return predictions_by_name[name][next(i for i,r in enumerate(records) if r["source"] == source)]["prediction"]
    assert prediction("composed","сняли свод памяти") == "сняли слот памяти"
    assert prediction("conflict","сняли свод памяти") == "сняли свод памяти"
    assert prediction("owner-high-conflict","сняли свод памяти") == "сняли слот памяти"
    assert prediction("owner-low","сняли свод памяти") == "сняли свод памяти"
    assert prediction("source-penalty","сняли свод памяти") == "сняли слот памяти"
    assert prediction("known-never","сняли свод памяти") == "сняли свод памяти"
    assert prediction("new-low","включи катэхэ") == "включи катэхэ"
    assert prediction("composed","после снятия сваты") == "после снятия сваты"
    assert prediction("composed","включи катэхи") == "включи катэхи"
    assert prediction("undo","сняли свуты") == "сняли слот"
    for a,b in [("composed","duplicate"),("manual-only","stale-settings")]:
        for filename in ["predictions.jsonl","metrics.json"]:
            assert (output/"benchmark"/a/filename).read_bytes() == (output/"benchmark"/b/filename).read_bytes()

    base = tomllib.loads(baseline.read_text())
    paths = {"config":baseline, **{k:baseline.parent/base[k] for k in ["dictionary","domain","ignore"]}, "frequency":baseline.parent/base["frequency"]["path"]}
    hashes = {k:sha(p) for k,p in paths.items()}; source_count=resource_count=0
    for path in output.rglob("*manifest.json"):
        m=json.loads(path.read_text()); source_count+=1
        for filename,digest in m["compiled_source_sha256"].items(): assert sha(root/filename)==digest
        assert sha(Path(m["input_file"]))==m["input_sha256"]
        resources=m["resources"]
        if path.name=="composition-manifest.json":
            assert sha(Path(m["bindings_file"]))==m["bindings_sha256"]
            assert sha(Path(m["decisions_file"]))==m["decisions_sha256"]
            input_name=path.parent.name
            ref_name="composed" if input_name in ["seed","stale-settings"] else input_name
            assert sha(output/"inputs"/f"{ref_name}.toml")==m["reference_config_sha256"]
            for filename,digest in m["output_sha256"].items(): assert sha(path.parent/filename)==digest
        elif "baseline_resources" in resources: resources=resources["baseline_resources"]
        if resources:
            for key,digest in resources["resource_sha256"].items(): assert hashes[key]==digest; resource_count+=1
    assert source_count == 66
    with (output/"comparison.csv").open("x",newline="",encoding="utf-8") as stream:
        writer=csv.DictWriter(stream,fieldnames=summary[0],lineterminator="\n");writer.writeheader();writer.writerows(summary)
    with (output/"case-comparison.csv").open("x",newline="",encoding="utf-8") as stream:
        writer=csv.writer(stream,lineterminator="\n");writer.writerow(["id","source","gold",*reports]);writer.writerows(comparison)
    save(output/"summary.json",summary);save(output/"commands.json",commands)
    save(output/"environment.json",dict(implementation_commit=subprocess.check_output(["git","-C",str(root),"rev-parse","HEAD"],text=True).strip(),
        implementation_status=subprocess.check_output(["git","-C",str(root),"status","--porcelain","--","src","examples","scripts","tests","configs","data"],text=True).splitlines(),
        binary_sha256=sha(binary),composer_binary_sha256=sha(composer),corpus_sha256=sha(corpus_path),
        verified_source_manifests=source_count,verified_resource_hashes=resource_count,
        source_sha256={n:sha(root/"scripts"/n) for n in ["composition_profiles.py","run_composition_experiment.py","prepare_user_phrase_fixture.py","prepare_draft_fixture.py","run_term_experiment.py"]},
        scope="offline composition, 20 synthetic dev controls, fresh explicit review per profile, runtime unchanged"))
    print(json.dumps(summary,ensure_ascii=False,indent=2))


if __name__=="__main__": main()
