#!/usr/bin/env python3
"""018: transactional publication, process fault tests and unchanged correction outputs."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve()
    root = Path(__file__).resolve().parents[1]
    previous = root / "experiments/017-profile-composition-v3"
    output.mkdir()
    for name in ["bundles", "requests", "snapshots", "configs", "benchmark"]:
        (output / name).mkdir()
    database_dir = Path(tempfile.mkdtemp(prefix="publication-018-", dir=root / "artifacts"))
    database = database_dir / "profiles.sqlite"
    commands = []

    def run(command, success=True, log=None):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, capture_output=True, text=True)
        if log:
            with (output / log).open("x") as stream:
                stream.write(result.stdout + result.stderr)
        assert (result.returncode == 0) == success, (command, result.stdout, result.stderr)
        return result

    cli_path = root / "publication/cli.mjs"

    def cli(*args, **kwargs):
        result = run(["node", cli_path, *args], **kwargs)
        return json.loads(result.stdout) if result.returncode == 0 else {"error": result.stderr.strip()}

    run(["node", "--test", "--test-reporter=tap", root / "publication/store.test.mjs"], log="node-tests.tap")
    run(["/home/user1/.local/bin/cargo", "test", "--manifest-path", root / "Cargo.toml", "--locked", "--offline"], log="rust-tests.log")
    bundles = {}
    for name in ["composed", "global-off"]:
        cli("pack", previous / "composed" / name, previous / "inputs" / f"{name}.toml", output / "bundles" / f"{name}.json")
        bundles[name] = json.loads((output / "bundles" / f"{name}.json").read_text())
    cli("create", database, "technical")
    receipts = []

    def request(name, revision, bundle):
        path = output / "requests" / f"{name}.json"
        save(path, dict(schema_version=1, request_id=name, expected_revision=revision, bundle=bundle))
        receipts.append(dict(operation="request", result=cli("request", database, path)))

    def publish(name, success=True):
        result = cli("publish", database, name, success=success)
        receipts.append(dict(operation="publish", request_id=name, result=result))
        return result

    def snapshot(name):
        value = cli("snapshot", database)
        save(output / "snapshots" / f"{name}.json", value)
        return value

    request("initial", 0, bundles["composed"]); publish("initial")
    first = snapshot("initial"); assert first["active"]["generation"] == 1
    request("older", 1, bundles["composed"])
    request("disable", 2, bundles["global-off"])
    assert "superseded" in publish("older", False)["error"]
    publish("disable"); disabled = snapshot("disabled")
    assert disabled["active"]["generation"] == 3
    invalid = copy.deepcopy(bundles["composed"]); invalid["config"] += "\n"
    request("invalid", 3, invalid)
    assert "config hash mismatch" in publish("invalid", False)["error"]
    assert snapshot("after-invalid")["active"] == disabled["active"]
    assert publish("initial")["replayed"]
    assert snapshot("after-old-receipt")["active"] == disabled["active"]
    request("restore", 4, bundles["composed"]); publish("restore")
    restored = snapshot("restored"); assert restored["active"]["generation"] == 5
    assert restored["active"]["bundle"] == first["active"]["bundle"]
    checks = []
    binary = root / "target/release/rechprav"
    corpus = previous / "inputs/corpus.jsonl"
    for name, snap in [("composed", first), ("global-off", disabled)]:
        bundle = snap["active"]["bundle"]
        validation = snap["active"]["validation"]
        for filename, digest in validation["compiled_source_sha256"].items():
            assert sha(root / filename) == digest
        assert sha(root / "examples/profile_validate.rs") == validation["validator_source_sha256"]
        assert sha(root / "src/profile_validation.rs") == validation["validation_source_sha256"]
        config = output / "configs" / f"{name}.toml"
        with config.open("x") as stream: stream.write(bundle["config"])
        assert sha(config) == validation["config_sha256"]
        run([binary, "terms-benchmark", corpus, "--config", config, "--output-dir", output / "benchmark" / name])
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (output / "benchmark" / name / filename).read_bytes() == (previous / "benchmark" / name / filename).read_bytes()
        checks.append(dict(profile=name, generation=snap["active"]["generation"],
                           config_sha256=sha(config), predictions_sha256=sha(output / "benchmark" / name / "predictions.jsonl"),
                           predictions_equal_017=True, metrics_equal_017=True))
    save(output / "summary.json", dict(profiles=checks, active_generation=5, desired_revision=5,
        process_crash_points=6, tests=18, quality_claim="unchanged 017 dev controls; no new ASR quality claim"))
    save(output / "receipts.json", receipts)
    save(output / "commands.json", commands)
    files = [root / "examples/profile_validate.rs", root / "src/profile_validation.rs", Path(__file__).resolve(), *sorted((root / "publication").rglob("*.mjs"))]
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", root, "status", "--porcelain", "--", "src", "examples", "scripts", "publication"], text=True).splitlines(),
        node=subprocess.check_output(["node", "--version"], text=True).strip(), database_path=str(database),
        source_sha256={str(p.relative_to(root)): sha(p) for p in files},
        runtime_binary_sha256=sha(binary), validator_binary_sha256=sha(root / "target/release/examples/profile_validate"),
        composer_binary_sha256=sha(root / "target/release/examples/feedback_compose"), corpus_sha256=sha(corpus)))
    print(json.dumps(checks, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
