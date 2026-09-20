#!/usr/bin/env python3
"""Measure resource loading in fresh processes and preserve exact B2 behavior."""
import argparse
import hashlib
import json
import platform
import random
import re
import statistics
import subprocess
from pathlib import Path


def sha(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as file:
        json.dump(value, file, ensure_ascii=False, indent=2)
        file.write("\n")


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_dir",type=Path)
    parser.add_argument("--profile",action="append",required=True,help="name=config.toml; may repeat")
    parser.add_argument("--repetitions",type=int,default=3)
    args=parser.parse_args()
    if not 1<=args.repetitions<=20: raise ValueError("require 1..20 fresh processes per profile")
    profiles={}
    for item in args.profile:
        name,config=item.split("=",1)
        if not re.fullmatch("[a-z0-9-]+",name) or name in profiles:
            raise ValueError("invalid or duplicate profile name")
        profiles[name]=Path(config).resolve()
    root=Path(__file__).resolve().parents[1]
    binary=root/"target/release/rechprav"
    profiler=root/"target/release/examples/profile"
    corpus=root/"data/fixtures/baseline/corpus.jsonl"
    validation=json.loads(subprocess.check_output([str(binary),"validate",str(corpus)]))
    if validation["real_asr"] or validation["pending"] or validation["ambiguous"]:
        raise ValueError("expected reviewed synthetic corpus")
    output=args.output_dir.resolve(); output.mkdir()
    commands=[]; metadata={}; results={name:[] for name in profiles}

    def run(command):
        command=list(map(str,command)); commands.append(command)
        result=subprocess.run(command,capture_output=True,text=True)
        if result.returncode:
            save(output/"failure.json",{"command":command,"stderr":result.stderr})
            result.check_returncode()
        return result.stdout

    def verify_sources(data):
        for name,expected in data["compiled_source_sha256"].items():
            if sha(root/name)!=expected: raise ValueError(f"stale source: {name}")

    for name,config in profiles.items():
        (output/f"{name}.toml").write_bytes(config.read_bytes())
        run([binary,"benchmark",corpus,"--config",config,"--output-dir",output/name])
        manifest=json.loads((output/name/"manifest.json").read_text()); verify_sources(manifest)
        if manifest["input_sha256"]!=sha(corpus): raise ValueError("corpus changed")
        for file in ["metrics.json","predictions.jsonl"]:
            if (output/name/file).read_bytes()!=(root/"experiments/003-frequency/b2-full"/file).read_bytes():
                raise ValueError(f"{name}: B2 {file} changed")
        metadata[name]=manifest["resources"]
    rng=random.Random(505)
    order=[]
    for repetition in range(args.repetitions):
        names=list(profiles); rng.shuffle(names)
        for name in names:
            order.append(name)
            report=json.loads(run([profiler,profiles[name],corpus,"--repetitions","1"]))
            verify_sources(report)
            if report["resources"]!=metadata[name]: raise ValueError("resources changed between processes")
            phases=report["load_phases_ns"]
            leaf_sum=sum(ns for phase,ns in phases.items() if phase!="total" and ".validation." not in phase)
            if leaf_sum>phases["total"]: raise ValueError("phase accounting overlap")
            if sum(ns for phase,ns in phases.items() if phase.startswith("dictionary.validation."))>phases["dictionary.decode"]:
                raise ValueError("dictionary phase accounting overlap")
            save(output/f"{name}-load-{repetition+1}.json",report)
            results[name].append(report)
            print(name,repetition+1,round(report["resource_load_ms"],2),
                  {p:round(ns/1e6,2) for p,ns in phases.items() if ns>=1e7},flush=True)
    summary={}
    for name,runs in results.items():
        fields=runs[0]["load_phases_ns"].keys()
        memory=lambda r,field: int(next(line.split()[1] for line in r["process_memory_linux"] if line.startswith(field+":")))
        summary[name]={"load_ms":[r["resource_load_ms"] for r in runs],
            "median_load_ms":statistics.median(r["resource_load_ms"] for r in runs),
            "median_phases_ms":{p:statistics.median(r["load_phases_ns"][p] for r in runs)/1e6 for p in fields},
            "rss_kib":[memory(r,"VmRSS") for r in runs],"peak_rss_kib":[memory(r,"VmHWM") for r in runs]}
    save(output/"summary.json",summary)
    save(output/"commands.json",commands)
    save(output/"validation.json",validation)
    save(output/"environment.json",{"git_revision":subprocess.check_output(["git","-C",str(root),"rev-parse","HEAD"],text=True).strip(),
         "implementation_status":subprocess.check_output(["git","-C",str(root),"status","--porcelain","--","src","examples","scripts","tests","configs"],text=True).splitlines(),
         "platform":platform.platform(),"cpu":next(line.split(":",1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")),
         "binary_sha256":sha(binary),"profiler_sha256":sha(profiler),"runner_sha256":sha(Path(__file__)),"corpus_sha256":sha(corpus),
         "profile_config_paths":{name:str(path) for name,path in profiles.items()},"seed":505,"run_order":order,
         "scope":"fresh processes; OS cache not flushed; config snapshots retain paths relative to originals; phase medians are not additive; B2 predictions/metrics exactly equal 003"})


if __name__=="__main__": main()
