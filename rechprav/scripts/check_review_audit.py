#!/usr/bin/env python3
"""Check review artifacts/resources and explain identical additional outputs."""
import argparse
from collections import Counter
import json
from pathlib import Path
import tomllib

from run_term_experiment import save, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve()
    root = Path(__file__).resolve().parents[1]
    environment = json.loads((output / "environment.json").read_text())
    assert sha(root / "target/release/rechprav") == environment["runtime_sha256"]
    assert sha(root / "scripts/run_review_audit.py") == environment["script_sha256"]
    assert sha(root / "scripts/run_term_experiment.py") == environment["helper_sha256"]
    inventory = json.loads((output / "data-audit.json").read_text())
    for entry in inventory["files"]:
        assert sha(root / entry["path"]) == entry["sha256"]
    resource_checks = 0
    manifests = list(output.rglob("manifest.json"))
    assert len(manifests) == 21
    # Hash each large resource once; compare with every manifest that used it.
    hashes = {}

    def check(path, expected):
        nonlocal resource_checks
        path = path.resolve()
        if path not in hashes:
            hashes[path] = sha(path)
        assert hashes[path] == expected, str(path)
        resource_checks += 1

    for path in manifests:
        manifest = json.loads(path.read_text())
        check(Path(manifest["input_file"]), manifest["input_sha256"])
        for name, digest in manifest["compiled_source_sha256"].items():
            check(root / name, digest)
        resources = manifest["resources"]
        args = manifest["arguments"]
        check(Path(args[args.index("--config") + 1]), resources["config_sha256"])
        baseline_path = Path(resources["effective_config"]["baseline_config"])
        baseline = tomllib.loads(baseline_path.read_text())
        paths = {"config": baseline_path}
        for name in ["dictionary", "domain", "ignore"]:
            paths[name] = baseline_path.parent / baseline[name]
        paths["frequency"] = baseline_path.parent / baseline["frequency"]["path"]
        for name, digest in resources["baseline_resources"]["resource_sha256"].items():
            check(paths[name], digest)
    candidates = {}
    outputs = {}
    for profile in environment["config_sha256"]:
        path = output / "additional-controls" / profile / "predictions.jsonl"
        records = [json.loads(line) for line in path.read_text().splitlines()]
        gates = Counter(candidate["gate"] for row in records for decision in row["decisions"]
                        for candidate in decision["candidates"])
        candidates[profile] = {"candidate_appearances": sum(gates.values()), "gates": dict(gates)}
        outputs[profile] = [(row["id"], row["prediction"], row["edits"]) for row in records]
    assert all(value == outputs["edit-2"] for value in outputs.values())
    save(output / "candidate-audit.json", {
        "dataset": "additional-controls", "profiles": candidates,
        "all_final_text_and_edits_equal": True,
        "scope": "candidate appearances in decision traces, not unique terms or applied edits; not equivalence of algorithms",
    })
    save(output / "checks.json", {
        "manifests": len(manifests), "hash_comparisons": resource_checks,
        "distinct_hashed_paths": len(hashes), "inventory_files_verified": len(inventory["files"]),
        "runtime_and_runner_hashes_verified": True,
        "all_additional_final_text_and_edits_equal": True,
        "verification_script_sha256": sha(Path(__file__).resolve()),
    })
    print((output / "checks.json").read_text())
    print((output / "candidate-audit.json").read_text())


if __name__ == "__main__":
    main()
