#!/usr/bin/env python3
"""Check per-language resources, String Catalogs and optional Swift extraction."""
import argparse
import collections
import json
import pathlib
import plistlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
RESOURCES = ROOT / "src/dmsg_ios/Resources"
LANGUAGES = ("en", "zh-Hans", "zh-Hant", "fr", "es", "ru", "ar", "hi", "id", "pt-BR", "ja", "de", "ko", "it", "tr", "vi", "th", "fil", "ms", "bn", "ur", "pl", "nl")
FORMAT = re.compile(r"%(?:\d+\$)?(?:ll|l)?[udi@f]|\$\{\w+\}")
PLURAL_FORMS = {"zero", "one", "two", "few", "many", "other"}


def check_localizable(source_strings, source_plurals, strings, plurals):
    """Validate one language against the English table, including plural rules."""
    problems = []
    for kind, source, translated in (("strings", source_strings, strings),
                                      ("plurals", source_plurals, plurals)):
        for key in source.keys() - translated.keys():
            problems.append(f"{kind}: missing {key}")
        for key in translated.keys() - source.keys():
            problems.append(f"{kind}: unexpected {key}")
    for key in strings.keys() & plurals.keys():
        problems.append(f"Key exists in both strings and plurals: {key}")
    for key, value in strings.items():
        if not isinstance(value, str) or not value.strip():
            problems.append(f"Incomplete {key}")
        elif key in source_strings and collections.Counter(FORMAT.findall(value)) != collections.Counter(FORMAT.findall(source_strings[key])):
            problems.append(f"Format mismatch {key}")
    for key in source_plurals.keys() & plurals.keys():
        source, translated = source_plurals[key], plurals[key]
        if not isinstance(translated, dict) or translated.keys() != source.keys():
            problems.append(f"Plural variables mismatch {key}")
            continue
        for variable, expected in source.items():
            actual = translated[variable]
            if variable == "NSStringLocalizedFormatKey":
                if actual != expected:
                    problems.append(f"Plural format mismatch {key}")
                continue
            if not isinstance(actual, dict):
                problems.append(f"Invalid plural rule {key}/{variable}")
                continue
            for field in ("NSStringFormatSpecTypeKey", "NSStringFormatValueTypeKey"):
                if actual.get(field) != expected[field]:
                    problems.append(f"Plural type mismatch {key}/{variable}/{field}")
            forms = {name: value for name, value in actual.items() if not name.startswith("NSString")}
            if "other" not in forms or forms.keys() - PLURAL_FORMS:
                problems.append(f"Invalid plural forms {key}/{variable}")
            parameters = collections.Counter(FORMAT.findall(expected["other"]))
            for form, value in forms.items():
                if not isinstance(value, str) or not value.strip():
                    problems.append(f"Incomplete plural {key}/{variable}/{form}")
                    continue
                used = collections.Counter(FORMAT.findall(value))
                # Zero/one/two can express the count in words (for example Arabic).
                if used - parameters or (form not in {"zero", "one", "two"} and used != parameters):
                    problems.append(f"Plural format mismatch {key}/{variable}/{form}")
    return problems


def localizable_tables(problems):
    directory = RESOURCES / "Localizations"
    tables = {}
    actual_languages = {path.stem for path in directory.glob("*.lproj")}
    if actual_languages != set(LANGUAGES):
        problems.append("Localizations folders do not match LANGUAGES")
    for lang in LANGUAGES:
        folder = directory / f"{lang}.lproj"
        tables[lang] = tuple(plistlib.loads((folder / f"Localizable.{kind}").read_bytes())
                             for kind in ("strings", "stringsdict"))
    source_strings, source_plurals = tables["en"]
    for lang, (strings, plurals) in tables.items():
        problems.extend(f"Localizable/{lang}: {problem}" for problem in
                        check_localizable(source_strings, source_plurals, strings, plurals))
    return source_strings.keys() | source_plurals.keys()

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
    catalogs = {"Localizable": localizable_tables(problems)}
    for file in [*RESOURCES.glob("*.xcstrings"), *(ROOT / "src/dmsg_ios/Extensions").rglob("*.xcstrings")]:
        catalog = json.loads(file.read_text())["strings"]
        if file.parent == RESOURCES:
            catalogs[file.stem] = catalog
        for key, entry in catalog.items():
            # Swift extraction can mark keys used by custom views as stale;
            # Xcode still ships them, so they need the same complete translations.
            if entry.get("shouldTranslate") is False:
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
                if "stringUnit" in translated:
                    source = entry.get("localizations", {}).get("en", {}).get("stringUnit", {}).get("value", key)
                    expected = collections.Counter(FORMAT.findall(source))
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
    for language in ("zh" if lang == "zh-Hans" else lang for lang in LANGUAGES):
        policy = ROOT / f"src/dmsg_frontend/src/lib/legal/privacy/{language}.md"
        if policy.read_bytes() != (RESOURCES / f"privacy-{language}.md").read_bytes():
            problems.append(f"Offline privacy policy differs: {language}")
    if problems:
        raise SystemExit("\n".join(sorted(set(problems))))
    print(f"{len(LANGUAGES)} localizations: app, extensions, formats, plurals and offline privacy copies verified.")

if __name__ == "__main__":
    main()
