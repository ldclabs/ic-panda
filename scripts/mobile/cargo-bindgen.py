#!/usr/bin/env python3
"""Run Cargo for the host UniFFI bindings generator."""
import json
import os
from pathlib import Path
import subprocess
import sys


def bindgen_config():
    cargo_home = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")
    registry_src = (cargo_home / "registry/src").absolute()
    if registry_src == registry_src.resolve():
        return []

    # Askama canonicalizes askama.toml before computing its path relative to
    # the crate source. Compile the same locked bindgen crate from its real path.
    host = subprocess.check_output(["rustc", "--print", "host-tuple"], text=True).strip()
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--format-version", "1",
        "--features", "dmsg_mobile/bindgen", "--filter-platform", host,
    ]))
    paths = []
    for package in metadata["packages"]:
        if package["name"] == "uniffi_bindgen":
            source = Path(package["manifest_path"]).parent
            resolved = source.resolve(strict=True)
            if source != resolved:
                paths.append(str(resolved))
    return ["--config", "paths=" + json.dumps(paths, ensure_ascii=False)] if paths else []


if __name__ == "__main__":
    os.execvp("cargo", ["cargo", *bindgen_config(), *sys.argv[1:]])
