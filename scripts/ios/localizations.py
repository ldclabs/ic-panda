#!/usr/bin/env python3
"""Check shipped translations and the compiler's optional Swift extraction."""
import argparse
import collections
import json
import pathlib
import re
from urllib.parse import urlsplit

ROOT = pathlib.Path(__file__).resolve().parents[2]
RESOURCES = ROOT / "src/dmsg_ios/Resources"
LANGUAGES = ("en", "zh-Hans", "fr", "es", "ru", "ar")
FORMAT = re.compile(r"%(?:\d+\$)?(?:ll|l)?[udi@f]")

def units(value):
    if isinstance(value, dict):
        if "stringUnit" in value:
            yield value["stringUnit"]
        for key, child in value.items():
            if key != "stringUnit":
                yield from units(child)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--derived-data", type=pathlib.Path)
    args = parser.parse_args()
    problems = []
    catalogs = {}
    for file in RESOURCES.glob("*.xcstrings"):
        catalog = json.loads(file.read_text())["strings"]
        catalogs[file.stem] = catalog
        for key, entry in catalog.items():
            if entry.get("extractionState") == "stale" or entry.get("shouldTranslate") is False:
                continue
            for lang in LANGUAGES:
                translated = entry.get("localizations", {}).get(lang)
                if not translated:
                    if lang != "en":
                        problems.append(f"{file.name}: {lang}: missing {key}")
                    continue
                for unit in units(translated):
                    if not unit.get("value") or unit.get("state") != "translated":
                        problems.append(f"{file.name}: {lang}: incomplete {key}")
                # Plurals may legitimately omit the numeric argument for one/two.
                if "stringUnit" in translated and file.stem == "Localizable":
                    expected = collections.Counter(FORMAT.findall(key))
                    actual = collections.Counter(FORMAT.findall(translated["stringUnit"]["value"]))
                    if expected != actual:
                        problems.append(f"{file.name}: {lang}: format mismatch {key}")
    if args.derived_data:
        files = list(args.derived_data.rglob("*.stringsdata"))
        if not files:
            problems.append("No Swift extraction found; build with SWIFT_EMIT_LOC_STRINGS=YES")
        for file in files:
            value = json.loads(file.read_text())
            source = value.get("source", "")
            if "/src/dmsg_ios/" not in source or "/Tests/" in source or "/UITests/" in source or "/CoreBridge/" in source:
                continue
            for table, strings in value.get("tables", {}).items():
                for entry in strings:
                    if entry["key"] not in catalogs.get(table, {}):
                        problems.append(f"{table}: missing extracted key {entry['key']}")
    for language in ("en", "zh", "fr", "es", "ru", "ar"):
        policy = ROOT / f"src/dmsg_frontend/src/lib/legal/privacy/{language}.md"
        if policy.read_bytes() != (RESOURCES / f"privacy-{language}.md").read_bytes():
            problems.append(f"Offline privacy policy differs: {language}")
    if problems:
        raise SystemExit("\n".join(sorted(set(problems))))
    print("Six-language catalogs, formats and offline privacy copies verified.")

if __name__ == "__main__":
    main()
