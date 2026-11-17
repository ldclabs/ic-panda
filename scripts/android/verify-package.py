#!/usr/bin/env python3
"""Verify packaged native code; APKs also require uncompressed ZIP page alignment."""
import argparse
import hashlib
import json
import struct
import zipfile
from pathlib import Path


def inspect(path, required, allow_ui_fixtures=False):
    bundle = path.suffix.lower() == ".aab"
    raw = path.read_bytes()
    libraries = []
    fixture_entries = []
    with zipfile.ZipFile(path) as archive:
        for entry in archive.infolist():
            if entry.filename.endswith((".dex", ".so")):
                data = archive.read(entry)
                if any(marker in data for marker in (b"open_ui_fixture", b"openUiFixture", b"WorkspaceFlowTest",
                                                     b"QaWorkspaceActivity", b"Sample-only password", b"Scale-only secret")):
                    fixture_entries.append(entry.filename)
            parts = entry.filename.split("/")
            native_path = parts[1:] if bundle else parts
            if len(native_path) != 3 or native_path[0] != "lib" or not native_path[-1].endswith(".so"):
                continue
            data = archive.read(entry)
            if data[:6] != b"\x7fELF\x02\x01":
                raise ValueError("Expected 64-bit little-endian ELF: " + entry.filename)
            machine = struct.unpack_from("<H", data, 18)[0]
            if machine != {"arm64-v8a": 183, "x86_64": 62}.get(native_path[1]):
                raise ValueError("ELF machine does not match packaged ABI: " + entry.filename)
            offset = struct.unpack_from("<Q", data, 32)[0]
            size, count = struct.unpack_from("<HH", data, 54)
            aligns = []
            for index in range(count):
                p = offset + size * index
                if struct.unpack_from("<I", data, p)[0] == 1:
                    alignment = struct.unpack_from("<Q", data, p + 48)[0]
                    if alignment < 16384:
                        raise ValueError("ELF LOAD alignment below 16 KiB: " + entry.filename)
                    aligns.append(alignment)
            if not aligns:
                raise ValueError("No loadable segments: " + entry.filename)
            name_length, extra_length = struct.unpack_from("<HH", raw, entry.header_offset + 26)
            data_offset = entry.header_offset + 30 + name_length + extra_length
            if not bundle and (entry.compress_type != zipfile.ZIP_STORED or data_offset % 16384):
                raise ValueError("Expected uncompressed 16 KiB ZIP alignment: " + entry.filename)
            libraries.append({"path": entry.filename, "sha256": hashlib.sha256(data).hexdigest(), "machine": machine, "load_alignment": aligns,
                              "zip_offset": data_offset if not bundle else None})
    observed = {item["path"].split("/")[-2] for item in libraries}
    if not set(required).issubset(observed):
        raise ValueError("Missing requested ABIs: " + ",".join(sorted(set(required) - observed)))
    if fixture_entries and not allow_ui_fixtures:
        raise ValueError("UI fixture code in ordinary package: " + ",".join(fixture_entries))
    return {"bundle" if bundle else "apk": path.name, "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw), "libraries": libraries,
            "ui_fixture_entries": fixture_entries, "ui_fixtures_allowed": allow_ui_fixtures,
            "scope": "Static bundle ELF validation only; generated APK splits need ZIP alignment and runtime checks."
                if bundle else "Static ELF/ZIP validation only; runtime, R8 and hardware require separate checks."}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("package", type=Path, help="APK or AAB")
    parser.add_argument("--abis", default="arm64-v8a,x86_64")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--allow-ui-fixtures", action="store_true", help="Only for an explicitly isolated QA package")
    args = parser.parse_args()
    report = json.dumps(inspect(args.package, args.abis.split(","), args.allow_ui_fixtures), indent=2) + "\n"
    if args.output:
        args.output.write_text(report)
    print(report)
