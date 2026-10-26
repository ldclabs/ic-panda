#!/usr/bin/env python3
"""Bind generated Swift and both native slices to source, toolchain and profile."""
import argparse
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
BRIDGE = ROOT / "src/dmsg_ios/CoreBridge"
MANIFEST = BRIDGE / "Artifacts/build.json"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def source_hash():
    # Hash the actual local dependency closure, not a hand-maintained crate list.
    metadata = json.loads(command("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    packages = {pathlib.Path(p["manifest_path"]).parent.resolve(): p for p in metadata["packages"]}
    entry = next(path for path, package in packages.items() if package["name"] == "dmsg_mobile")
    pending, selected = [entry], set()
    while pending:
        path = pending.pop()
        if path in selected:
            continue
        if not path.is_relative_to(ROOT):
            raise SystemExit("A local native dependency is outside the reviewed repository")
        selected.add(path)
        for dependency in packages[path]["dependencies"]:
            if dependency.get("path") and dependency.get("kind") != "dev":
                candidate = pathlib.Path(dependency["path"]).resolve()
                if candidate not in packages:
                    raise SystemExit("A local native dependency is absent from workspace metadata")
                pending.append(candidate)
    roots = [str(p.relative_to(ROOT)) for p in sorted(selected)]
    roots += ["Cargo.toml", "Cargo.lock", ".cargo", "rust-toolchain.toml",
              "scripts/ios/build-core.sh", "scripts/ios/artifacts.py"]
    paths = set(command("git", "ls-files", "--cached", "--others", "--exclude-standard", "--", *roots).splitlines())
    h = hashlib.sha256()
    for name in sorted(paths):
        path = ROOT / name
        if path.is_file():
            h.update(name.encode() + b"\\0" + digest(path).encode() + b"\\n")
    return h.hexdigest()


def artifacts():
    return [ROOT / "Cargo.lock", BRIDGE / "Generated/DMsgCore.swift", *sorted((BRIDGE / "Artifacts/DMsgCoreFFI.xcframework").rglob("*.a")), *sorted((BRIDGE / "Headers").glob("*"))]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["record", "verify", "fingerprint"])
    parser.add_argument("--profile", choices=["debug", "release"])
    parser.add_argument("--source-sha256")
    args = parser.parse_args()
    if args.action == "fingerprint":
        print(source_hash())
        return
    if args.source_sha256 and args.source_sha256 != source_hash():
        raise SystemExit("Rust sources changed during the build; rebuild before recording artifacts")
    if args.action == "record":
        if not args.profile:
            parser.error("record requires --profile")
        files = artifacts()
        if len([p for p in files if p.suffix == ".a"]) != 2:
            raise SystemExit("Expected device and simulator libraries")
        value = {"schema": 1, "commit": command("git", "rev-parse", "HEAD"), "source_sha256": source_hash(),
                 "profile": args.profile, "rustc": command("rustc", "--version"), "xcode": command("xcodebuild", "-version"),
                 "artifacts": {str(p.relative_to(ROOT)): digest(p) for p in files}}
        MANIFEST.write_text(json.dumps(value, indent=2) + "\n")
    else:
        value = json.loads(MANIFEST.read_text())
        if value["source_sha256"] != source_hash():
            raise SystemExit("Rust sources differ from the native artifacts; rebuild the core")
        if args.profile and value["profile"] != args.profile:
            raise SystemExit(f"Expected {args.profile} native libraries, found {value['profile']}")
        for name, expected in value["artifacts"].items():
            if digest(ROOT / name) != expected:
                raise SystemExit(f"Artifact changed: {name}")
        print(f"Verified {value['profile']} native artifacts and source fingerprint")


if __name__ == "__main__":
    main()
