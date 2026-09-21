#!/usr/bin/env python3
"""Record exact public source/build hashes for cross-repository integration."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True, type=Path)
parser.add_argument("--fixtures", type=Path)
args = parser.parse_args()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


files = {root / p for p in [
    "Cargo.toml", "Cargo.lock", "dfx.json", "Makefile", "scripts/test-dmsg.sh",
    "scripts/verify-dmsg-vectors.mjs", "scripts/verify-commerce-vectors.mjs",
    "scripts/export-commerce-snapshot.py", "src/dmsg_app/scripts/bindings.mjs",
    "docs/protocol/README.md", "docs/protocol/commerce.md", "docs/protocol/commerce.cddl",
]}
for package in ["dmsg_types", "dmsg_protocol", "dmsg_runtime", "dmsg_user", "dmsg_cose",
                "dmsg_payment", "membership", "dmsg_commerce"]:
    for path in (root / "src" / package).rglob("*"):
        if path.is_file() and path.suffix in {".rs", ".toml", ".did", ".json"}:
            files.add(path)
for path in (root / "src/dmsg_app/src/lib/canisters/generated").rglob("*"):
    if path.is_file():
        files.add(path)
for package in ["dmsg_integration", "dmsg_test_ledger", "dmsg_test_sns"]:
    files.update(p for p in (root / "tests" / package).rglob("*")
                 if p.is_file() and p.suffix in {".rs", ".toml"})

wasm_dir = Path(os.environ.get("DMSG_WASM_DIR", str(
    Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target"))) /
    "wasm32-unknown-unknown/release")))
result = {
    "schema": 1,
    "profiles": ["membership/1", "dmsg-commerce/1", "delivery/2", "cose-execution/3"],
    "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    "source_state": "working-tree snapshot; hashes identify these files, not a new Git commit",
    "files_sha256": {str(p.relative_to(root)): sha(p) for p in sorted(files)},
    "wasm_sha256": {name: sha(wasm_dir / (name + ".wasm")) for name in [
        "dmsg_user", "dmsg_handle", "dmsg_cose", "dmsg_payment", "membership",
        "dmsg_commerce", "dmsg_test_ledger", "dmsg_test_sns"]},
}
if args.fixtures:
    result["fixtures_sha256"] = {p.name: sha(p) for p in sorted(args.fixtures.glob("*.cbor"))}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(result, indent=2) + "\n")
print(f"Wrote {len(files)} public source hashes and {len(result['wasm_sha256'])} Wasm hashes to {args.output}")
