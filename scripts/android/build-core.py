#!/usr/bin/env python3
"""Build the same fingerprinted mobile core for Kotlin and every requested ABI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "src/dmsg_android/app"
GENERATED = APP / "build/generated/dmsg"
NDK_VERSION = "28.2.13676358"
TARGETS = {"arm64-v8a": ("aarch64-linux-android", "aarch64-linux-android31"),
           "x86_64": ("x86_64-linux-android", "x86_64-linux-android31")}


def file_hash(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def library_name(abi):
    return f"jniLibs/{abi}/libdmsg_mobile.so"


def run(args, env=None):
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def sources():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT))
    packages = {p["name"]: p for p in metadata["packages"]}
    names, pending = set(), ["dmsg_mobile"]
    while pending:
        name = pending.pop()
        if name in names:
            continue
        names.add(name)
        pending.extend(d["name"] for d in packages[name]["dependencies"]
                       if d.get("path") and d["name"] in packages)
    files = {ROOT / n for n in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")}
    for name in names:
        directory = Path(packages[name]["manifest_path"]).parent
        files.update(p for p in directory.rglob("*") if p.is_file()
                     and p.suffix in {".rs", ".toml", ".json", ".cddl"}
                     and not {"target", "build", "node_modules"}.intersection(p.relative_to(directory).parts))
    # Only producers affect native bytes. Test runners/package inspectors consume
    # artifacts and must not invalidate every ABI when their checks change.
    files.update(ROOT / "scripts/android" / name for name in ("build-core.py", "build-core.sh"))
    files.update(p for p in (ROOT / "tests/vectors").rglob("*") if p.is_file())
    return sorted(files)


def fingerprint(files, fixtures=False):
    digest = hashlib.sha256()
    digest.update(b"ui-fixtures" if fixtures else b"production")
    # The same sources built with another compiler are not interchangeable.
    digest.update(subprocess.check_output(["rustc", "-vV"], cwd=ROOT))
    digest.update(NDK_VERSION.encode())
    sdk = os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT"))
    if sdk:
        ndk = Path(os.environ.get("ANDROID_NDK_HOME", Path(sdk) / "ndk" / NDK_VERSION))
        digest.update((ndk / "source.properties").read_bytes())
    for path in files:
        digest.update(str(path.relative_to(ROOT)).encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bindings-only", action="store_true")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--native-only", action="store_true")
    parser.add_argument("--fixtures", action="store_true", help="Isolated QA library; never use for release")
    parser.add_argument("--abis", default=os.environ.get("DMSG_ANDROID_ABIS", "arm64-v8a,x86_64"))
    args = parser.parse_args()
    global GENERATED
    GENERATED = APP / ("build/generated/dmsg-qa" if args.fixtures else "build/generated/dmsg")
    abis = args.abis.split(",")
    if not abis or any(a not in TARGETS for a in abis):
        parser.error("ABIs must be arm64-v8a and/or x86_64")
    inputs = sources()
    build_id = fingerprint(inputs, args.fixtures)
    manifest = GENERATED / "manifest.json"
    state = json.loads(manifest.read_text()) if manifest.exists() else {}
    recorded = state.get("outputs", {})
    matching = state.get("source") == build_id
    def valid(name):
        path = GENERATED / name
        return name in recorded and path.is_file() and file_hash(path) == recorded[name]

    bindings = {str(p.relative_to(GENERATED)) for p in (GENERATED / "kotlin").rglob("*") if p.is_file()}
    required = {"kotlin/net/dmsg/core/BuildIdentity.kt", "kotlin/net/dmsg/core/dmsg_mobile.kt"}
    bindings_valid = (matching and required <= bindings
                      and bindings == {name for name in recorded if name.startswith("kotlin/")}
                      and all(valid(name) for name in bindings))
    if args.bindings_only and not args.verify and bindings_valid:
        print("Reusing verified dMsg core " + build_id)
        return
    built = [abi for abi in TARGETS if matching and abi in state.get("abis", []) and valid(library_name(abi))]
    requested_valid = set(abis).issubset(built)
    recorded_valid = set(state.get("abis", [])) == set(built)
    if args.verify:
        if not matching:
            raise SystemExit("Stale native/Kotlin build: source fingerprint changed")
        if not bindings_valid:
            raise SystemExit("Missing or stale Kotlin bindings")
        if not requested_valid or not recorded_valid:
            raise SystemExit("Missing or stale native ABI")
        print("Verified dMsg core " + build_id)
        return
    if bindings_valid and requested_valid and recorded_valid:
        print("Reusing verified dMsg core " + build_id)
        return
    if args.native_only and not bindings_valid:
        raise SystemExit("Generate current Kotlin bindings before native slices")
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target/android")).resolve()
    if args.fixtures:
        target = target / "ui-fixtures"
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), DMSG_MOBILE_BUILD_ID=build_id)
    # Fingerprinted per-target settings below are authoritative, including CI.
    env.pop("RUSTFLAGS", None)
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    if not bindings_valid:
        run(["cargo", "build", "--locked", "-p", "dmsg_mobile", "--features", "bindgen,ui-fixtures" if args.fixtures else "bindgen"], env)
        suffix = "dylib" if platform.system() == "Darwin" else "so"
        GENERATED.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="bindings-", dir=GENERATED) as temp:
            run([str(target / "debug/dmsg-mobile-bindgen"), "generate", "--library",
                 str(target / ("debug/libdmsg_mobile." + suffix)), "--language", "kotlin",
                 "--no-format", "--out-dir", temp], env)
            destination = GENERATED / "kotlin"
            if destination.exists():
                shutil.rmtree(destination)
            shutil.move(temp, destination)
        package = GENERATED / "kotlin/net/dmsg/core"
        (package / "BuildIdentity.kt").write_text(
            'package net.dmsg.core\n\ninternal object BuildIdentity { const val HASH = "' + build_id + '" }\n')
    if not args.bindings_only:
        sdk = os.environ.get("ANDROID_HOME", os.environ.get("ANDROID_SDK_ROOT"))
        if not sdk:
            raise SystemExit("Set ANDROID_HOME to the Android SDK")
        ndk = Path(os.environ.get("ANDROID_NDK_HOME", Path(sdk) / "ndk" / NDK_VERSION))
        host = "darwin-x86_64" if platform.system() == "Darwin" else "linux-x86_64"
        toolchain = ndk / "toolchains/llvm/prebuilt" / host / "bin"
        for abi in abis:
            if abi in built:
                continue
            triple, compiler = TARGETS[abi]
            name = triple.replace("-", "_")
            native_env = dict(env)
            native_env.update({f"CC_{name}": str(toolchain / (compiler + "-clang")),
                f"AR_{name}": str(toolchain / "llvm-ar"),
                f"CARGO_TARGET_{name.upper()}_LINKER": str(toolchain / (compiler + "-clang")),
                f"CARGO_TARGET_{name.upper()}_RUSTFLAGS": "-C link-arg=-Wl,-z,max-page-size=16384"})
            run(["cargo", "build", "--locked", "-p", "dmsg_mobile", "--lib", "--target", triple, "--profile", "mobile"]
                + (["--features", "ui-fixtures"] if args.fixtures else []), native_env)
            output = GENERATED / "jniLibs" / abi
            output.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target / triple / "mobile/libdmsg_mobile.so", output / "libdmsg_mobile.so")
            recorded[library_name(abi)] = file_hash(output / "libdmsg_mobile.so")
            built.append(abi)
    if fingerprint(sources(), args.fixtures) != build_id:
        raise SystemExit("Sources changed during build; rerun before packaging")
    outputs = {str(p.relative_to(GENERATED)): file_hash(p) for p in (GENERATED / "kotlin").rglob("*") if p.is_file()}
    outputs.update({library_name(abi): recorded[library_name(abi)] for abi in built})
    with tempfile.NamedTemporaryFile(mode="w", dir=GENERATED, delete=False) as draft:
        json.dump({"source": build_id, "fixtures": args.fixtures, "abis": built, "outputs": outputs}, draft, indent=2)
        draft.write("\n")
    os.replace(draft.name, manifest)
    print("Built dMsg core " + build_id + " for " + ",".join(built))


if __name__ == "__main__":
    main()
