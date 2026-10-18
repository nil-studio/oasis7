#!/usr/bin/env python3
"""Run the prebuilt archive_memory example in fresh processes; preserve raw data.

Build first: ./scripts/cargo-dev.sh build -p oasis7 --no-default-features --example archive_memory
Run before/after with the same compiler/profile and this identical tool:
  python3 crates/oasis7/examples/measure_archive_memory.py /tmp/archive-before
The default dev profile measures requested allocation accurately; timings must
only be compared within the same profile. macOS time -l reports peak RSS bytes;
Linux time -v reports maximum resident set size in KiB. Neither is live heap.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=pathlib.Path)
parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("target/debug/examples/archive_memory"))
parser.add_argument("--repeats", type=int, default=3)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
binary = args.binary.resolve(strict=True)
def output(command):
    return subprocess.check_output(command, text=True).strip()
metadata = {
    "commit": output(["git", "rev-parse", "HEAD"]),
    "machine": platform.platform(), "architecture": platform.machine(),
    "rust": output(["rustc", "-Vv"]), "allocator": "std::alloc::System with requested-byte meter",
    "profile": "dev (wrapper: debug=0, incremental=0)", "features": "--no-default-features",
    "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "tool_sha256": hashlib.sha256(pathlib.Path("crates/oasis7/examples/archive_memory.rs").read_bytes()).hexdigest(),
    "host_source_sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(pathlib.Path("crates/oasis7/src/simulator").glob("kernel_host*.rs"))},
    "repeats": args.repeats,
    "build": "./scripts/cargo-dev.sh build -p oasis7 --no-default-features --example archive_memory",
    "source_status": output(["git", "status", "--short"]),
}
(args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
subprocess.run(["git", "diff", "--", "crates/oasis7/src/simulator", "crates/oasis7/examples/archive_memory.rs"],
               stdout=(args.output / "implementation.diff").open("w"), check=True)
for mode, extension in [("native", "empty"), ("host", "empty"), ("host", "extended")]:
    for repeat in range(args.repeats):
        name = f"{mode}-{extension}-{repeat + 1}"
        time_option = "-l" if platform.system() == "Darwin" else "-v"
        command = ["/usr/bin/time", time_option, str(binary), mode, extension]
        with (args.output / f"{name}.jsonl").open("w") as stdout, (args.output / f"{name}.time").open("w") as stderr:
            subprocess.run(command, stdout=stdout, stderr=stderr, check=True)
        print(name, flush=True)
