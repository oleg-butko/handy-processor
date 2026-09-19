#!/usr/bin/env python3
"""Export independent word forms and frequencies OFFLINE from pinned packages.

No evaluation corpus is read. A new output directory is required. The full
dictionary includes rare/name/variant forms; it is not a spelling authority.
"""
import argparse
import hashlib
import importlib.metadata as metadata
import json
import platform
import re
import time
from pathlib import Path


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    started = time.perf_counter()
    packages = {}
    for line in Path(__file__).with_name("requirements-resources.txt").read_text().splitlines():
        if line and not line.startswith("#"):
            name, expected = line.split("==")
            actual = metadata.version(name)
            if actual != expected:
                raise ValueError(f"{name}: expected {expected}, found {actual}")
            packages[name] = actual
    import pymorphy3
    import wordfreq

    output = args.output_dir
    output.mkdir()
    morph = pymorphy3.MorphAnalyzer()
    russian = re.compile(r"[а-яё]{1,128}\Z")
    counts = {"raw_keys": 0, "duplicate_keys": 0, "excluded_keys": 0, "word_forms": 0}
    previous = None
    with (output / "dictionary.txt").open("x", encoding="utf-8", newline="\n") as file:
        for word in morph.dictionary.words.iterkeys():
            counts["raw_keys"] += 1
            if previous is not None and word < previous:
                raise ValueError("DAWG iteration is not sorted; export requires sorted keys")
            if word == previous:
                counts["duplicate_keys"] += 1
                continue
            previous = word
            if not russian.fullmatch(word):
                counts["excluded_keys"] += 1
                continue
            file.write(word + "\n")
            counts["word_forms"] += 1
    print(json.dumps(counts), flush=True)
    frequencies = wordfreq.get_frequency_dict("ru", "large")
    counts["frequency_raw_entries"] = len(frequencies)
    counts["frequency_exported_entries"] = 0
    with (output / "frequency.tsv").open("x", encoding="utf-8", newline="\n") as file:
        for word, probability in sorted(frequencies.items()):
            if russian.fullmatch(word):
                file.write(f"{word}\t{probability * 1e9:.12g}\n")
                counts["frequency_exported_entries"] += 1
    inputs = {}
    dictionary_path = Path(morph.dictionary.path)
    for path in sorted(dictionary_path.iterdir()):
        if path.is_file():
            inputs[f"pymorphy3-dicts-ru/{path.name}"] = sha256(path)
    frequency_path = Path(wordfreq.available_languages("large")["ru"])
    inputs[f"wordfreq/{frequency_path.name}"] = sha256(frequency_path)
    # Preserve upstream package metadata (including data license statements).
    for name in ["pymorphy3-dicts-ru", "wordfreq"]:
        distribution = metadata.distribution(name)
        (output / f"{name}-METADATA.txt").write_text(distribution.read_text("METADATA"), encoding="utf-8")
        license_text = distribution.read_text("LICENSE.txt")
        if license_text:
            (output / f"{name}-LICENSE.txt").write_text(license_text, encoding="utf-8")
    manifest = {
        "schema_version": 1,
        "python": platform.python_version(), "packages": packages,
        "script_sha256": sha256(Path(__file__)),
        "counts": counts, "opencorpora": morph.dictionary.meta,
        "input_sha256": inputs,
        "output_sha256": {name: sha256(output / name) for name in ["dictionary.txt", "frequency.tsv"]},
        "elapsed_seconds": time.perf_counter() - started,
        "dictionary": {"source": "https://pypi.org/project/pymorphy3-dicts-ru/2.4.417150.4580142/",
                       "data_license": "CC BY-SA 3.0", "attribution": "OpenCorpora; pymorphy3-dicts-ru by Danylo Halaiko",
                       "filter": "all stored forms matching lowercase [а-яё]{1,128}; no corpus-derived additions"},
        "frequency": {"source": "https://pypi.org/project/wordfreq/3.1.1/",
                      "attribution": "Robyn Speer, wordfreq; upstream NOTICE includes source attributions",
                      "data_license": "CC BY-SA 4.0", "code_license": "Apache-2.0",
                      "unit": "occurrences per billion", "list": "ru large",
                      "normalization": "spellings exported separately; runtime е/ё collisions use max, not sum",
                      "missing": "unknown; zero scoring floor, not proof of invalidity"},
        "scope": "Offline resource export; no corpus input, no quality claim. See docs/rechprav/resource-sources.md.",
    }
    (output / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest["output_sha256"], indent=2))


if __name__ == "__main__":
    main()
