#!/usr/bin/env python3
"""Generate/check iOS template SVG assets from the desktop's Remix Icon paths."""
import argparse
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
DESKTOP = ROOT / "src/dmsg_desktop/src/renderer"
IOS = ROOT / "src/dmsg_ios"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    glyphs = dict(re.findall(r"^  '?([\w-]+)'?: '([^']+)', //", (DESKTOP / "components/glyphs.ts").read_text(), re.M))
    source = (IOS / "Features/AppGlyph.swift").read_text()
    names = re.findall(r"^    case (\w+)$", source, re.M)
    if not names or len(names) != len(set(names)):
        raise SystemExit("AppGlyph must declare unique icon cases, one per line")
    desktop_types = (DESKTOP / "views/vault/vault.ts").read_text().split("export const typeIcon:", 1)[1].split("}", 1)[0]
    expected_types = dict(re.findall(r"^  (\w+): '(\w+)'", desktop_types, re.M))
    ios_types = dict(re.findall(r'^        case "(\w+)": \.(\w+)$', source, re.M))
    if ios_types != expected_types:
        raise SystemExit("AppGlyph.vaultItem must match the desktop vault typeIcon mapping")
    files = {}
    for name in names:
        path = glyphs[name]
        folder = IOS / f"Resources/Assets.xcassets/Glyph-{name}.imageset"
        files[folder / "icon.svg"] = (
            '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">\n'
            '  <!-- Generated from desktop glyphs.ts; Remix Icon 4.6.0, Apache-2.0. -->\n'
            f'  <path fill="#000" d="{path}"/>\n</svg>\n'
        )
        files[folder / "Contents.json"] = json.dumps({
            "images": [{"filename": "icon.svg", "idiom": "universal"}],
            "info": {"author": "xcode", "version": 1},
            "properties": {"preserves-vector-representation": True, "template-rendering-intent": "template"},
        }, indent=2) + "\n"
    files[IOS / "Resources/Remix-LICENSE.txt"] = (DESKTOP / "public/licenses/Remix-LICENSE.txt").read_text()
    stale = []
    for path, expected in files.items():
        if args.check:
            if not path.exists() or path.read_text() != expected:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expected)
    expected_assets = {f"Glyph-{name}.imageset" for name in names}
    for path in (IOS / "Resources/Assets.xcassets").glob("Glyph-*.imageset"):
        if path.name not in expected_assets:
            stale.append(f"Unused asset: {path.relative_to(ROOT)}")
    if stale:
        raise SystemExit("Icons out of sync; run scripts/ios/sync-icons.py:\n" + "\n".join(stale))
    print(f"{len(names)} iOS business icons {'verified against' if args.check else 'generated from'} desktop paths.")


if __name__ == "__main__":
    main()
