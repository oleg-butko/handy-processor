#!/usr/bin/env python3
"""Review audit: exact-source overlap and fixed 013 costs on other authored controls."""
import argparse
from collections import Counter, defaultdict
import csv
import json
from pathlib import Path
import subprocess
from run_term_experiment import save, sha


def rows(path):
    return [json.loads(s) for s in path.read_text().splitlines()]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve(); output.mkdir()
    root = Path(__file__).resolve().parents[1]
    binary = root / "target/release/rechprav"
    old = root / "experiments/013-weighted"
    fixture_names = ["baseline", "terms", "term-context", "term-spans", "pronunciation", "user-phrases",
                     "term-families", "feedback", "weighted", "feedback-drafts"]
    paths = [root / "data/fixtures/corpus.jsonl", root / "data/user-examples/2026-09-21/corpus.jsonl",
             *[root / "data/fixtures" / name / "corpus.jsonl" for name in fixture_names],
             root / "experiments/020-mixed-generation/inputs/corpus.jsonl"]
    inventory = []; by_source = defaultdict(list)
    for path in paths:
        records = rows(path); relative = str(path.relative_to(root))
        inventory.append(dict(path=relative, sha256=sha(path), records=len(records),
                              unique_sources=len({r["source"] for r in records}),
                              gold_spans=sum(len(r["edits"]) for r in records),
                              keep_rows=sum(not r["edits"] for r in records),
                              provenance=dict(Counter(r["provenance"]["kind"] for r in records))))
        for r in records:
            by_source[r["source"]].append(dict(path=relative, id=r["id"], gold=r["gold"], split=r["split"]))
    conflicts = {s: entries for s, entries in by_source.items() if len({r["gold"] for r in entries}) > 1}
    save(output / "data-audit.json", dict(files=inventory, total_rows=sum(r["records"] for r in inventory),
         unique_sources=len(by_source), repeated_sources=sum(len(v) > 1 for v in by_source.values()),
         conflicts=conflicts, repeated_groups={s:v for s,v in by_source.items() if len(v) > 1},
         scope="exact source bytes only; authored examples are not independent ASR observations"))
    reference = root / "data/fixtures/weighted/corpus.jsonl"
    user = root / "data/user-examples/2026-09-21/corpus.jsonl"
    seen = {r["source"] for path in [reference, user] for r in rows(path)}
    selected = []; exclusions = []
    for path in [root / "data/fixtures/user-phrases/corpus.jsonl", root / "data/fixtures/term-families/corpus.jsonl"]:
        for r in rows(path):
            reason = "conflicting_gold" if r["source"] in conflicts else "already_in_reference_or_selected" if r["source"] in seen else None
            if reason: exclusions.append(dict(path=str(path.relative_to(root)), id=r["id"], source=r["source"], reason=reason))
            else: selected.append(r); seen.add(r["source"])
    assert len(selected) == 57 and sum(len(r["edits"]) for r in selected) == 21
    transfer = output / "additional-controls.jsonl"
    with transfer.open("x") as stream:
        for r in selected: stream.write(json.dumps(r, ensure_ascii=False) + "\n")
    save(output / "selection.json", dict(excluded=exclusions, records=57, gold_spans=21,
         selection="exact-source deduplication and removal of reference/user sources; no prediction-dependent filtering",
         limitation="010/011 were available at design time; this is not a held-out test"))
    configs = ["edit-1", "edit-2", "weighted-uniform", "weighted-pairs", "weighted-vowels", "weighted-wide", "weighted-gaps"]
    datasets = {"reference-013":reference, "user-reported":user, "additional-controls":transfer}
    commands = []; summary = []; changes = {}; cached = {}

    def run(command):
        command = list(map(str, command)); commands.append(command)
        p = subprocess.run(command, capture_output=True, text=True)
        if p.returncode:
            save(output / "failure.json", dict(command=command, stderr=p.stderr)); p.check_returncode()
        return p.stdout

    signature = lambda e: (e["byte_start"], e["byte_end"], e["replacement"])
    for dataset, corpus in datasets.items():
        records = rows(corpus)
        (output / dataset).mkdir()
        json.loads(run([binary, "validate", corpus]))
        for profile in configs:
            directory = output / dataset / profile
            run([binary, "terms-benchmark", corpus, "--config", old / "configs" / f"{profile}.toml", "--output-dir", directory])
            predictions = rows(directory / "predictions.jsonl"); cached[dataset, profile] = predictions
            tp = fp = fn = 0
            for gold, prediction in zip(records, predictions, strict=True):
                assert gold["id"] == prediction["id"] and prediction["complete"]
                expected = {signature(e) for e in gold["edits"]}; got = {signature(e) for e in prediction["edits"]}
                tp += len(expected & got); fp += len(got-expected); fn += len(expected-got)
            metrics = json.loads((directory / "metrics.json").read_text())
            assert (tp, fp, fn) == tuple(metrics["span_counts"][k] for k in ["tp", "fp", "fn_"])
            if dataset != "additional-controls":
                original = old / ("new-weighted" if dataset == "reference-013" else "user-reported") / profile
                for filename in ["predictions.jsonl", "metrics.json"]: assert (directory / filename).read_bytes() == (original / filename).read_bytes()
            summary.append(dict(dataset=dataset, profile=profile, records=len(records), tp=tp, fp=fp, fn=fn,
                                harmed_correct_input=metrics["span_counts"]["harmed_correct_input"],
                                exact_records=metrics["exact_records"], wer_after=metrics["wer_after"]))
            print(summary[-1], flush=True)
        with (output / dataset / "case-comparison.csv").open("x", newline="") as stream:
            writer = csv.writer(stream); writer.writerow(["id", "source", "gold", *configs])
            for i, row in enumerate(records): writer.writerow([row["id"], row["source"], row["gold"], *[cached[dataset, c][i]["prediction"] for c in configs]])
        for profile in configs:
            deltas = []
            for gold, before, after in zip(records, cached[dataset, "edit-2"], cached[dataset, profile], strict=True):
                expected = {signature(e) for e in gold["edits"]}
                old_edits = {signature(e) for e in before["edits"]}; new_edits = {signature(e) for e in after["edits"]}
                if old_edits != new_edits:
                    deltas.append(dict(id=gold["id"], source=gold["source"], gold=gold["gold"], before=before["prediction"], after=after["prediction"],
                        gained_tp=len((new_edits & expected)-old_edits), lost_tp=len((old_edits & expected)-new_edits),
                        new_fp=len((new_edits-expected)-old_edits), removed_fp=len((old_edits-expected)-new_edits)))
            changes[f"{dataset}/{profile}"] = deltas
    save(output / "summary.json", summary); save(output / "changes-vs-edit-2.json", changes)
    with (output / "comparison.csv").open("x", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=summary[0]); writer.writeheader(); writer.writerows(summary)
    for path in output.rglob("manifest.json"):
        m = json.loads(path.read_text())
        assert sha(Path(m["input_file"])) == m["input_sha256"]
        for name, digest in m["compiled_source_sha256"].items(): assert sha(root / name) == digest
    save(output / "commands.json", commands)
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", root, "status", "--porcelain", "--", "src", "scripts", "tests", "examples", "configs"], text=True).splitlines(),
        runtime_sha256=sha(binary), script_sha256=sha(Path(__file__).resolve()), helper_sha256=sha(root / "scripts/run_term_experiment.py"),
        config_sha256={p:sha(old / "configs" / f"{p}.toml") for p in configs},
        corpus_sha256={name:sha(path) for name,path in datasets.items()}, historical_profiles_equal=14,
        claim="frozen settings, retrospective authored controls, no runtime edits or threshold fitting; not an independent ASR test"))


if __name__ == "__main__": main()
