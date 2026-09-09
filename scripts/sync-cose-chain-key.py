#!/usr/bin/env python3
"""Sync the unreleased upstream crate, or verify the checked-in snapshot."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "vendor/ic_cose_chain_key"
FILES = ("Cargo.toml", "README.md", "LICENSE-APACHE", "LICENSE-MIT", "src/lib.rs")


def checksum(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sync", action="store_true", help="replace snapshot from upstream checkout")
    parser.add_argument("--source", type=Path, default=Path(os.environ.get("IC_COSE_DIR", ROOT.parent / "ic-cose")))
    args = parser.parse_args()
    if args.sync:
        source = args.source.resolve()
        crate = source / "src/ic_cose_chain_key"
        contents = {name: (crate / name).read_bytes() for name in FILES}
        base = subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip()
        for name, data in contents.items():
            path = TARGET / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        (TARGET / "SOURCE.json").write_text(json.dumps({
            "repository": "https://github.com/ldclabs/ic-cose",
            "path": "src/ic_cose_chain_key",
            "upstream_base_commit": base,
            "unreleased_working_tree": True,
            "sha256": {name: checksum(data) for name, data in contents.items()},
        }, indent=2) + "\n")
    record = json.loads((TARGET / "SOURCE.json").read_text())
    if set(record["sha256"]) != set(FILES):
        raise SystemExit("unexpected source file list")
    for name in FILES:
        if checksum((TARGET / name).read_bytes()) != record["sha256"][name]:
            raise SystemExit(f"upstream snapshot changed: {name}; edit upstream and resync")
    print("IC COSE chain-key snapshot verified")


if __name__ == "__main__":
    main()
