#!/usr/bin/env python3
"""Compare ZIP levels on the full source allowlist, retaining integrity checks."""

import json
import sys
import tempfile
import time
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import create_source_archive as source_archive


def main():
    output = Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    entries = source_archive.collect_sources()
    generated = {source_archive.BUILD_NUMBER_FILE: f"{source_archive.resolve_build_number()}\n".encode("ascii")}
    names = {name for _, name in entries}.union(generated)
    report = {"uncompressed_bytes": sum(p.stat().st_size for p, _ in entries if p.is_file()),
              "entries": len(names), "runs": []}
    with tempfile.TemporaryDirectory(prefix="handy-zip-bench-") as temporary:
        for level in (1, 3, 6, 9):
            path = Path(temporary) / f"level-{level}.zip"
            start = time.perf_counter()
            with ZipFile(path, "x", compression=ZIP_DEFLATED, compresslevel=level) as archive:
                for source, name in entries:
                    archive.write(source, arcname=name)
                for name, content in generated.items():
                    archive.writestr(name, content)
            written = time.perf_counter()
            source_archive.validate_archive(path, names, generated)
            end = time.perf_counter()
            run = {"level": level, "write_seconds": written - start,
                   "validate_seconds": end - written, "total_seconds": end - start,
                   "zip_bytes": path.stat().st_size}
            report["runs"].append(run)
            output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
            print(json.dumps(run), flush=True)


if __name__ == "__main__":
    main()
