#!/bin/zsh
# Copy the screenshots a UI test run attached (VisualBaselineTests, App Store and
# other captures) out of its result bundle, named after their attachments.
#   scripts/ios/export-screenshots.sh <run.xcresult> <output directory>
set -euo pipefail
if (( $# != 2 )); then
  print -u2 "usage: $0 <run.xcresult> <output directory>"
  exit 64
fi
result=$1 output=$2
raw=$(mktemp -d "${TMPDIR:-/tmp}/dmsg-shots.XXXXXX")
trap 'rm -rf "$raw"' EXIT
xcrun xcresulttool export attachments --path "$result" --output-path "$raw" >/dev/null
mkdir -p "$output"
python3 - "$raw" "$output" <<'PY'
import json, pathlib, re, shutil, sys
raw, output = map(pathlib.Path, sys.argv[1:])
count = 0
for test in json.loads((raw / "manifest.json").read_text()):
    for attachment in test["attachments"]:
        name = attachment.get("suggestedHumanReadableName", attachment["exportedFileName"])
        source = raw / attachment["exportedFileName"]
        if source.suffix.lower() not in (".png", ".jpeg", ".jpg", ".heic"):
            continue
        # Drop the "_0_<UUID>" suffix Xcode adds, keep the attachment name.
        stem = re.sub(r"_\d+_[0-9A-F-]{36}$", "", pathlib.Path(name).stem)
        shutil.copyfile(source, output / f"{re.sub(r'[^A-Za-z0-9._-]+', '-', stem)}{source.suffix.lower()}")
        count += 1
print(f"{count} screenshots in {output}")
PY
