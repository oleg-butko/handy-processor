#!/usr/bin/env python3
"""019: single-thread engine ownership/reload and publication validation regression."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    with path.open("x") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    output = parser.parse_args().output_dir.resolve(); output.mkdir()
    root = Path(__file__).resolve().parents[1]
    publication = root / "experiments/018-profile-publication"
    previous = root / "experiments/017-profile-composition-v3"
    commands = []

    def run(command, log, stdin=None):
        command = list(map(str, command)); commands.append(command)
        result = subprocess.run(command, input=stdin, capture_output=True, text=True)
        with (output / log).open("x") as stream: stream.write(result.stdout)
        with (output / (log + ".stderr")).open("x") as stream: stream.write(result.stderr)
        assert result.returncode == 0, result.stderr
        return result.stdout

    rust = run(["/home/user1/.local/bin/cargo", "test", "--manifest-path", root / "Cargo.toml", "--locked", "--offline"], "rust-tests.log")
    rust_count = sum(map(int, re.findall(r"test result: ok\. (\d+) passed", rust)))
    assert rust_count == 142
    node = run(["node", "--test", "--test-reporter=tap", root / "publication/store.test.mjs"], "publication-tests.tap")
    assert "# pass 18" in node and "# fail 0" in node
    snapshots = {name: json.loads((publication / "snapshots" / f"{name}.json").read_text()) for name in ["initial", "disabled", "restored"]}
    corpus_path = previous / "inputs/corpus.jsonl"
    corpus = [json.loads(line) for line in corpus_path.read_text().splitlines()]
    predictions = {name: [json.loads(line) for line in (previous / "benchmark" / name / "predictions.jsonl").read_text().splitlines()]
                   for name in ["composed", "global-off"]}
    messages = []; expected = []; comparisons = []

    def send(command, ok=True, **checks):
        messages.append(command); expected.append(dict(ok=ok, **checks))

    def analyses(handle, profile, generation, phase):
        for record, prediction in zip(corpus, predictions[profile], strict=True):
            send(dict(kind="analyze", id=handle, text=record["source"]), prediction=prediction, generation=generation, phase=phase)

    send(dict(kind="status"), active=None)
    send(dict(kind="acquire", id="missing"), False, error="no loaded profile")
    send(dict(kind="reload", snapshot=snapshots["initial"]), changed=True, active=1)
    send(dict(kind="acquire", id="old")); analyses("old", "composed", 1, "initial")
    send(dict(kind="reload", snapshot=snapshots["initial"]), changed=False, active=1)
    send(dict(kind="acquire", id="old"), False, error="duplicate handle")
    send(dict(kind="reload", snapshot=snapshots["disabled"]), changed=True, active=3)
    send(dict(kind="acquire", id="off"))
    analyses("old", "composed", 1, "old_after_reload"); analyses("off", "global-off", 3, "disabled")
    send(dict(kind="reload", snapshot=snapshots["initial"]), False, error="regression", active=3)
    invalid = json.loads(json.dumps(snapshots["restored"])); invalid["active"]["receipt"]["generation"] = 99
    send(dict(kind="reload", snapshot=invalid), False, error="receipt/validation", active=3)
    send(dict(kind="reload", snapshot=snapshots["restored"]), changed=True, active=5)
    send(dict(kind="acquire", id="restored")); analyses("restored", "composed", 5, "restored")
    for handle in ["old", "off", "restored"]: send(dict(kind="release", id=handle))
    send(dict(kind="analyze", id="old", text="свод"), False, error="unknown handle", active=5)
    send(dict(kind="status"), active=5)
    stdin = "".join(json.dumps(m, ensure_ascii=False) + "\n" for m in messages)
    with (output / "input.jsonl").open("x") as stream: stream.write(stdin)
    binary = root / "target/release/examples/profile_reader"
    stdout = run([binary, "technical"], "output.jsonl", stdin)
    responses = [json.loads(line) for line in stdout.splitlines()]
    for response, checks in zip(responses, expected, strict=True):
        for name, digest in response["compiled_source_sha256"].items(): assert sha(root / name) == digest
        result = response["result"]
        assert result["ok"] == checks["ok"], result
        if "active" in checks: assert result["status"]["active_generation"] == checks["active"]
        if "error" in checks: assert checks["error"] in result["error"]
        if "changed" in checks: assert result["output"]["changed"] == checks["changed"]
        if "prediction" in checks:
            analysis = result["output"]["analysis"]; gold = checks["prediction"]
            assert result["output"]["generation"] == checks["generation"]
            assert analysis["corrected"] == gold["prediction"]
            for field in ["edits", "complete", "decisions", "comparisons"]: assert analysis[field] == gold[field]
            comparisons.append(dict(id=gold["id"], phase=checks["phase"], generation=checks["generation"], equal_017=True))
    assert len(comparisons) == 80
    save(output / "comparison.json", comparisons)
    save(output / "summary.json", dict(rust_tests=rust_count, reader_tests=7, publication_tests=18,
        requests=len(messages), analysis_comparisons=80, all_equal_017=True, final_generation=5,
        claim="single-thread ownership/interleaving and unchanged authored dev predictions; no multithreaded server claim"))
    save(output / "commands.json", commands)
    paths = ["examples/profile_reader.rs", "examples/profile_validate.rs", "src/profile_reader.rs",
             "src/profile_validation.rs", "tests/profile_reader.rs", "scripts/run_reader_experiment.py"]
    save(output / "environment.json", dict(
        implementation_commit=subprocess.check_output(["git", "-C", root, "rev-parse", "HEAD"], text=True).strip(),
        implementation_status=subprocess.check_output(["git", "-C", root, "status", "--porcelain", "--", "src", "examples", "tests", "scripts", "publication"], text=True).splitlines(),
        source_sha256={p: sha(root / p) for p in paths},
        input_snapshot_sha256={name: sha(publication / "snapshots" / f"{name}.json") for name in snapshots},
        corpus_sha256=sha(corpus_path), reader_binary_sha256=sha(binary),
        validator_binary_sha256=sha(root / "target/release/examples/profile_validate"),
        runtime_binary_sha256=sha(root / "target/release/rechprav"),
        composer_binary_sha256=sha(root / "target/release/examples/feedback_compose")))
    print(f"{rust_count} Rust tests, 18 publication tests, {len(comparisons)} unchanged analyses; active generation 5")


if __name__ == "__main__":
    main()
