#!/usr/bin/env python3
"""Build a packed trie; compare exact behavior and measure three fresh processes."""
import argparse
import hashlib
import heapq
import json
import platform
import statistics
import subprocess
import time
from pathlib import Path


def sha(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as file:
        json.dump(value, file, ensure_ascii=False, indent=2)
        file.write("\n")


def make_queries(dictionary):
    # Fixed min-hash sample by length, independent of source/gold and frequencies.
    heaps = [[] for _ in range(5)]
    for line in dictionary.open(encoding="utf-8"):
        word = line.strip()
        group = next((i for i, bound in enumerate([4, 8, 16, 32, 128]) if len(word) <= bound), None)
        if group is None:
            continue
        rank = int.from_bytes(hashlib.sha256(("rechprav-004:" + word).encode()).digest(), "big")
        heap = heaps[group]
        item = (-rank, word)
        if len(heap) < 16:
            heapq.heappush(heap, item)
        elif item > heap[0]:
            heapq.heapreplace(heap, item)
    queries = []

    def add(word, category, distance=1, candidates=20, nodes=100000):
        queries.append({"id": f"lookup-{len(queries):03}", "query":word, "category":category,
                        "distance":distance, "max_candidates":candidates, "max_nodes":nodes})

    selected = []
    for group, heap in enumerate(heaps):
        words = list(dict.fromkeys(word.replace("ё", "е") for _,word in sorted(heap, reverse=True)))[:8]
        if len(words) != 8:
            raise ValueError(f"insufficient unique forms in length band {group}")
        selected.extend(words)
        for i, word in enumerate(words):
            middle = len(word) // 2
            add(word, "known")
            add(word[:middle] + word[middle+1:], "deletion")
            add(word[:middle] + "а" + word[middle:], "insertion")
            substitute = "о" if word[middle] == "а" else "а"
            add(word[:middle] + substitute + word[middle+1:], "substitution")
            if len(word) > 1:
                position = min(middle, len(word)-2)
                swapped = word[:position] + word[position+1] + word[position] + word[position+2:]
                if swapped != word:
                    add(swapped, "transposition")
            if i == 0:
                add(word[:-1] + "аа", "distance_two_probe", distance=2)
    for word in ["маско", "драйвир", "елко", "", "🙂", "а"*128]:
        add(word,"edge_case")
    for word in ["маско", "а", "драйвир"]:
        for candidates, nodes in [(0,100000),(1,100000),(20,0),(20,1),(20,10)]:
            add(word,"limit_probe",candidates=candidates,nodes=nodes)
    return queries, selected


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir",type=Path)
    parser.add_argument("--index",type=Path,default=None,help="Reuse existing .rpt; otherwise build the standard path (must be new)")
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    binary=root/"target/release/rechprav"
    profiler=root/"target/release/examples/profile"
    source=root/"artifacts/opencorpora-wordfreq/dictionary.txt"
    frequencies=root/"artifacts/opencorpora-wordfreq/frequency.tsv"
    corpus=root/"data/fixtures/baseline/corpus.jsonl"
    source_hash=sha(source)
    resource_manifest=json.loads((source.parent/"manifest.json").read_text())
    if source_hash!=resource_manifest["output_sha256"]["dictionary.txt"] or sha(frequencies)!=resource_manifest["output_sha256"]["frequency.tsv"]:
        raise ValueError("resources do not match independent export manifest")
    validation=json.loads(subprocess.check_output([str(binary),"validate",str(corpus)]))
    if validation["real_asr"] or validation["pending"] or validation["ambiguous"]:
        raise ValueError("only reviewed technical fixtures are allowed")
    output=args.output_dir.resolve(); output.mkdir()
    commands=[]

    def run(command):
        commands.append(list(map(str,command)))
        process=subprocess.run(command,capture_output=True,text=True)
        if process.returncode:
            save(output/"failure.json",{"command":commands[-1],"stderr":process.stderr})
            process.check_returncode()
        return process.stdout

    def check_sources(manifest):
        for file, expected in manifest["compiled_source_sha256"].items():
            if sha(root/file)!=expected:
                raise ValueError(f"stale compiled source: {file}")

    index=(args.index.resolve() if args.index else source.with_suffix(".rpt"))
    if args.index:
        save(output/"index-build.json",{"reused":str(index),"index_sha256":sha(index)})
    else:
        start=time.perf_counter()
        built=json.loads(run([str(binary),"build-index",str(source),"--output",str(index)]))
        built["whole_process_seconds"]=time.perf_counter()-start
        if built["source_sha256"]!=source_hash or built["index_sha256"]!=sha(index):
            raise ValueError("built index provenance mismatch")
        save(output/"index-build.json",built)
    queries, selected=make_queries(source)
    save(output/"queries.json",queries)
    save(output/"query-generation.json",{"source_sha256":source_hash,"seed":"rechprav-004:",
         "method":"eight distinct normalized min-SHA256-ranked spellings per length band, deterministic edits and explicit limit probes",
         "length_bands":[[1,4],[5,8],[9,16],[17,32],[33,128]],"selected":selected,
         "quality_scope":"dictionary-derived technical probes, not ASR errors or gold labels"})
    configs={"reference":root/"configs/opencorpora-b2.toml", "packed":output/"packed.toml"}
    # Capture the effective experiment configuration without editing the reference.
    reference=configs["reference"].read_text()
    for profile in configs:
        content=reference.replace('"../artifacts/opencorpora-wordfreq/dictionary.txt"',json.dumps(str(index if profile=="packed" else source)))
        content=content.replace('"../artifacts/opencorpora-wordfreq/frequency.tsv"',json.dumps(str(frequencies)))
        content=content.replace('"../data/fixtures/baseline/domain.txt"',json.dumps(str(root/"data/fixtures/baseline/domain.txt")))
        content=content.replace('"../data/fixtures/baseline/ignore.txt"',json.dumps(str(root/"data/fixtures/baseline/ignore.txt")))
        config=output/f"{profile}.toml"
        with config.open("x") as file: file.write(content)
        configs[profile]=config
    semantic=None; summary={}
    for profile,config in configs.items():
        run([str(binary),"benchmark",str(corpus),"--config",str(config),"--output-dir",str(output/profile)])
        manifest=json.loads((output/profile/"manifest.json").read_text()); check_sources(manifest)
        if manifest["input_sha256"]!=sha(corpus): raise ValueError("corpus changed")
        if manifest["resources"]["resource_sha256"]["frequency"]!=resource_manifest["output_sha256"]["frequency.tsv"]:
            raise ValueError("frequency resource changed")
        if profile=="packed" and manifest["resources"]["dictionary_storage"]["source_sha256"]!=source_hash:
            raise ValueError("packed source identity mismatch")
        predictions=(output/profile/"predictions.jsonl").read_bytes()
        metrics=json.loads((output/profile/"metrics.json").read_text())
        old=root/"experiments/003-frequency/b2-full"
        if predictions!=(old/"predictions.jsonl").read_bytes() or metrics!=json.loads((old/"metrics.json").read_text()):
            raise ValueError(f"{profile}: changed B2 behavior relative to 003")
        runs=[]
        for repetition in range(3):
            measured=json.loads(run([str(profiler),str(config),str(corpus),"--queries",str(output/"queries.json")]))
            check_sources(measured)
            if measured["resources"]!=manifest["resources"]: raise ValueError("profiler resources differ")
            if measured["queries_sha256"]!=sha(output/"queries.json"): raise ValueError("queries changed")
            if semantic is None:
                semantic=measured["lookup_checks"]
                save(output/"lookup-results.json",semantic)
            elif measured["lookup_checks"]!=semantic:
                raise ValueError("lookup membership/candidates/distances/order/limits/visited nodes differ")
            measured.pop("lookup_checks")
            save(output/f"{profile}-performance-{repetition+1}.json",measured)
            runs.append(measured)
            print(profile,repetition+1,"load_ms",round(measured["resource_load_ms"],2),measured["process_memory_linux"],flush=True)
        memory=lambda run,field: int(next(line.split()[1] for line in run["process_memory_linux"] if line.startswith(field+":")))
        summary[profile]={"load_ms":[r["resource_load_ms"] for r in runs],
            "median_load_ms":statistics.median(r["resource_load_ms"] for r in runs),
            "rss_kib":[memory(r,"VmRSS") for r in runs],"peak_rss_kib":[memory(r,"VmHWM") for r in runs],
            "warm_analyze_p95_ns":[r["warm_record_p95_ns"] for r in runs],
            "lookup_statistics":[r["lookup_statistics"] for r in runs]}
    save(output/"summary.json",summary)
    save(output/"validation.json",validation)
    save(output/"commands.json",commands)
    save(output/"verification.json",{"exact_lookup_comparisons":len(queries)*6,
         "distinct_queries":len(queries),"all_lookup_snapshots_equal":True,
         "compared":"membership, canonical spellings, candidate keys, distances, order, scores, completeness, reasons and visited nodes",
         "b2_metrics_and_predictions_equal_003":True,"warm_cache_note":"OS cache not flushed; three separate processes per profile, profile order fixed"})
    save(output/"environment.json",{"git_revision":subprocess.check_output(["git","-C",str(root),"rev-parse","HEAD"],text=True).strip(),
         "implementation_status":subprocess.check_output(["git","-C",str(root),"status","--porcelain","--","src","tests","examples","scripts","configs"],text=True).splitlines(),
         "platform":platform.platform(),"python":platform.python_version(),
         "cpu":next(line.split(":",1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")),
         "binary_sha256":sha(binary),"profiler_sha256":sha(profiler),"runner_sha256":sha(Path(__file__)),
         "source_sha256":source_hash,"index_sha256":sha(index),"frequencies_sha256":sha(frequencies),"corpus_sha256":sha(corpus)})
    print(json.dumps({p:{k:v for k,v in s.items() if k!="lookup_statistics"} for p,s in summary.items()},indent=2))


if __name__=="__main__":
    main()
