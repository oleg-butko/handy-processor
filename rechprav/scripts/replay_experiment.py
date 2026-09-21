#!/usr/bin/env python3
"""Replay saved benchmark manifests with bounded parallel processes, compare exact outputs."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import subprocess
from pathlib import Path
from run_term_experiment import save, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("previous", type=Path); p.add_argument("output", type=Path)
    p.add_argument("--workers", type=int, choices=range(1,5), default=2)
    args = p.parse_args()
    root = Path(__file__).resolve().parents[1]
    previous = args.previous.resolve(); output = args.output.resolve(); output.mkdir()
    sources = sorted(previous.rglob("manifest.json"))
    assert sources, "no benchmark manifests"

    def replay(path):
        old = json.loads(path.read_text()); command = old["arguments"].copy()
        assert command[0] in ["benchmark", "terms-benchmark"], path
        target = output / path.parent.relative_to(previous); target.parent.mkdir(parents=True, exist_ok=True)
        assert old["input_sha256"] == sha(Path(old["input_file"]))
        config_path = Path(command[command.index("--config") + 1])
        metadata = old["resources"]
        digest = metadata["config_sha256"] if command[0] == "terms-benchmark" else metadata["resource_sha256"]["config"]
        assert sha(config_path) == digest
        command[command.index("--output-dir") + 1] = str(target)
        command = [str(root / "target/release/rechprav"), *command]
        completed = subprocess.run(command, capture_output=True, text=True, cwd=root.parent)
        if completed.returncode:
            raise RuntimeError(f"{path}: {completed.stderr}")
        for filename in ["predictions.jsonl", "metrics.json"]:
            assert (path.parent / filename).read_bytes() == (target / filename).read_bytes(), (path, filename)
        current = json.loads((target / "manifest.json").read_text())
        for file, digest in current["compiled_source_sha256"].items(): assert sha(root / file) == digest, file
        return {"previous_manifest": str(path), "manifest_sha256": sha(path), "command": command,
                "predictions_sha256": sha(target / "predictions.jsonl"), "metrics_sha256": sha(target / "metrics.json")}

    try:
        with ThreadPoolExecutor(max_workers=args.workers) as pool: results = list(pool.map(replay, sources))
    except Exception as error:
        save(output / "failure.json", {"error": str(error)})
        raise
    save(output / "replay.json", {"workers":args.workers, "byte_equal_profiles":len(results),
         "source":str(previous), "runner_sha256":sha(Path(__file__)), "runs":results})
    print(f"{len(results)} historical predictions/metrics reproduced byte-for-byte", flush=True)


if __name__ == "__main__": main()
