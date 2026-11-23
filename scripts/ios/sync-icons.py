#!/usr/bin/env python3
"""Generate/check the iOS and Android business icons from the shared Remix paths.

iOS gets custom SF Symbols (`Glyph-*.symbolset`), so the icons scale with text, sit on
the text baseline and take symbol effects. Android gets vector drawables.
"""
import argparse
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
DESKTOP = ROOT / "src/dmsg_desktop/src/renderer"
IOS = ROOT / "src/dmsg_ios"
ANDROID = ROOT / "src/dmsg_android/app/src/main"

# Filled versions for the tab bar, which shows filled symbols as the system tabs do.
# Remix Icon 4.9.1, Apache-2.0; the line paths match the desktop's 4.6.0 ones.
TAB_FILLS = {
    "vault": "M10.0049 19.9998H6.00488V21.9998H4.00488V19.9998H3.00488C2.4526 19.9998 2.00488 19.552 2.00488 18.9998V3.99977C2.00488 3.44748 2.4526 2.99977 3.00488 2.99977H10.0049V1.59C10.0049 1.31385 10.2287 1.09 10.5049 1.09C10.5324 1.09 10.5599 1.09227 10.5871 1.0968L21.1693 2.8605C21.6515 2.94086 22.0049 3.35805 22.0049 3.84689V5.99977H23.0049V7.99977H22.0049V14.9998H23.0049V16.9998H22.0049V19.1526C22.0049 19.6415 21.6515 20.0587 21.1693 20.139L20.0049 20.3331V21.9998H18.0049V20.6664L10.5871 21.9027C10.3147 21.9481 10.0571 21.7641 10.0117 21.4917C10.0072 21.4646 10.0049 21.4371 10.0049 21.4095V19.9998ZM12.0049 19.6388L20.0049 18.3055V4.69402L12.0049 3.36069V19.6388ZM16.5049 13.9998C15.6765 13.9998 15.0049 12.8805 15.0049 11.4998C15.0049 10.1191 15.6765 8.99977 16.5049 8.99977C17.3333 8.99977 18.0049 10.1191 18.0049 11.4998C18.0049 12.8805 17.3333 13.9998 16.5049 13.9998Z",  # safe-2-fill
    "channels": "M16.8 19L14 22.5L11.2 19H6C5.44772 19 5 18.5523 5 18V7.10256C5 6.55028 5.44772 6.10256 6 6.10256H22C22.5523 6.10256 23 6.55028 23 7.10256V18C23 18.5523 22.5523 19 22 19H16.8ZM2 2H19V4H3V15H1V3C1 2.44772 1.44772 2 2 2Z",  # discuss-fill
    "folder": "M12.4142 5H21C21.5523 5 22 5.44772 22 6V20C22 20.5523 21.5523 21 21 21H3C2.44772 21 2 20.5523 2 20V4C2 3.44772 2.44772 3 3 3H10.4142L12.4142 5Z",  # folder-fill
    "account": "M12 14V22H4C4 17.5817 7.58172 14 12 14ZM12 13C8.685 13 6 10.315 6 7C6 3.685 8.685 1 12 1C15.315 1 18 3.685 18 7C18 10.315 15.315 13 12 13ZM14.5946 18.8115C14.5327 18.5511 14.5 18.2794 14.5 18C14.5 17.7207 14.5327 17.449 14.5945 17.1886L13.6029 16.6161L14.6029 14.884L15.5952 15.4569C15.9883 15.0851 16.4676 14.8034 17 14.6449V13.5H19V14.6449C19.5324 14.8034 20.0116 15.0851 20.4047 15.4569L21.3971 14.8839L22.3972 16.616L21.4055 17.1885C21.4673 17.449 21.5 17.7207 21.5 18C21.5 18.2793 21.4673 18.551 21.4055 18.8114L22.3972 19.3839L21.3972 21.116L20.4048 20.543C20.0117 20.9149 19.5325 21.1966 19.0001 21.355V22.5H17.0001V21.3551C16.4677 21.1967 15.9884 20.915 15.5953 20.5431L14.603 21.1161L13.6029 19.384L14.5946 18.8115ZM18 17C17.4477 17 17 17.4477 17 18C17 18.5523 17.4477 19 18 19C18.5523 19 19 18.5523 19 18C19 17.4477 18.5523 17 18 17Z",  # user-settings-fill
}

NUMBER = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")
ARITY = {"M": 2, "L": 2, "T": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "A": 7, "Z": 0}
# Symbol template v3 guides at 100 pt: (baseline, cap line, size against medium). At medium
# a 24-unit Remix box becomes 96 units centred between baseline and cap line, so its
# 20 units of drawing sit just past both. SwiftUI's Label, in list rows and menus, shows
# nothing for a custom symbol that only has the medium scale.
SCALES = {"S": (696, 625.541, 0.783), "M": (1126, 1055.54, 1.0), "L": (1556, 1485.54, 1.29)}
SCALE = 4.0
LEFT = 1400.0
GUIDE = 'style="fill:none;stroke:#27AAE1;opacity:1;stroke-width:0.5;"'


def transform(d, scale, dx, dy):
    """Scale and move SVG path data, keeping its commands."""
    out, i, command = [], 0, None
    while i < len(d):
        char = d[i]
        if char.isspace() or char == ",":
            i += 1
            continue
        if char.isalpha():
            command = char
            out.append(char)
            i += 1
            if char in "Zz":
                continue
        elif command is None:
            raise SystemExit(f"Path data must start with a command: {d[:20]}")
        args = []
        for index in range(ARITY[command.upper()]):
            while i < len(d) and (d[i].isspace() or d[i] == ","):
                i += 1
            if command.upper() == "A" and index in (3, 4):
                args.append(d[i])  # arc flags may be written without separators
                i += 1
                continue
            match = NUMBER.match(d, i)
            args.append(float(match.group()))
            i = match.end()
        relative, kind = command.islower(), command.upper()

        def x(value):
            return value * scale + (0 if relative else dx)

        def y(value):
            return value * scale + (0 if relative else dy)

        if kind == "H":
            values = [x(args[0])]
        elif kind == "V":
            values = [y(args[0])]
        elif kind == "A":
            values = [args[0] * scale, args[1] * scale, args[2], args[3], args[4], x(args[5]), y(args[6])]
        else:
            values = [x(value) if index % 2 == 0 else y(value) for index, value in enumerate(args)]
        out.append(" ".join(value if isinstance(value, str) else f"{value:.3f}".rstrip("0").rstrip(".") for value in values))
        if command in "Mm":
            command = "l" if relative else "L"
    return " ".join(out)


def symbol(path):
    lines = "\n".join(f'  <line id="{name}" {GUIDE} x1="263" x2="3036" y1="{y}" y2="{y}"/>' for name, y in [
        ("Baseline-S", 696), ("Capline-S", 625.541), ("Baseline-M", 1126), ("Capline-M", 1055.54),
        ("Baseline-L", 1556), ("Capline-L", 1485.54)])
    margins, glyphs = [], []
    for scale, (baseline, capline, factor) in SCALES.items():
        size = SCALE * factor
        right = LEFT + 24 * size
        for side, x in (("left", LEFT), ("right", right)):
            margins.append(f'  <line id="{side}-margin-Regular-{scale}" {GUIDE} x1="{x:g}" x2="{x:g}" '
                           f'y1="{capline - 25:g}" y2="{baseline + 25:g}"/>')
        middle = (baseline + capline) / 2
        glyphs.append(f'  <g id="Regular-{scale}">\n   <path d="{transform(path, size, LEFT, middle - 12 * size)}"/>\n  </g>')
    margins, glyphs = "\n".join(margins), "\n".join(glyphs)
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="3300" height="2200">
 <!-- Generated by scripts/ios/sync-icons.py; Remix Icon, Apache-2.0. -->
 <g id="Notes">
  <rect height="2200" id="artboard" style="fill:white;opacity:1" width="3300" x="0" y="0"/>
  <text id="template-version" style="stroke:none;fill:black;font-family:sans-serif;font-size:13;text-anchor:end;" transform="matrix(1 0 0 1 3036 1933)">Template v.3.0</text>
 </g>
 <g id="Guides">
{lines}
{margins}
 </g>
 <g id="Symbols">
{glyphs}
 </g>
</svg>
"""


SYMBOL_CONTENTS = json.dumps({
    "info": {"author": "xcode", "version": 1},
    "symbols": [{"filename": "icon.svg", "idiom": "universal"}],
}, indent=2) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    glyphs = dict(re.findall(r"^  '?([\w-]+)'?: '([^']+)', //", (DESKTOP / "components/glyphs.ts").read_text(), re.M))
    source = (IOS / "Features/AppGlyph.swift").read_text()
    names = re.findall(r"^    case (\w+)$", source, re.M)
    if not names or len(names) != len(set(names)):
        raise SystemExit("AppGlyph must declare unique icon cases, one per line")
    if not set(TAB_FILLS) <= set(names):
        raise SystemExit("TAB_FILLS names an icon AppGlyph does not declare")
    desktop_types = (DESKTOP / "views/vault/vault.ts").read_text().split("export const typeIcon:", 1)[1].split("}", 1)[0]
    expected_types = dict(re.findall(r"^  (\w+): '(\w+)'", desktop_types, re.M))
    ios_types = dict(re.findall(r'^        case "(\w+)": \.(\w+)$', source, re.M))
    if ios_types != expected_types:
        raise SystemExit("AppGlyph.vaultItem must match the desktop vault typeIcon mapping")
    android_source = (ANDROID / "java/net/dmsg/android/ui/kit/Glyphs.kt").read_text()
    android_types = dict(re.findall(r'^    "(\w+)" -> R\.drawable\.glyph_(\w+)$', android_source, re.M))
    if {kind: glyph.replace("_", "-") for kind, glyph in android_types.items()} != expected_types:
        raise SystemExit("vaultGlyph in ui/kit/Glyphs.kt must match the desktop vault typeIcon mapping")
    files = {}
    for name in names:
        # Swift case names are camelCase; desktop keys are kebab-case.
        path = glyphs[re.sub(r"[A-Z]", lambda letter: "-" + letter.group().lower(), name)]
        # `.fill` is the name symbolVariant(.fill) and the tab bar look for.
        assets = [(f"Glyph-{name}", path)] + ([(f"Glyph-{name}.fill", TAB_FILLS[name])] if name in TAB_FILLS else [])
        for asset, data in assets:
            folder = IOS / f"Shared/Glyphs.xcassets/{asset}.symbolset"
            files[folder / "icon.svg"] = symbol(data)
            files[folder / "Contents.json"] = SYMBOL_CONTENTS
        drawable = ANDROID / f"res/drawable/glyph_{re.sub(r'[A-Z]', lambda letter: '_' + letter.group().lower(), name)}.xml"
        files[drawable] = (
            '<!-- Remix Icon 4.6.0, Apache-2.0; generated by scripts/ios/sync-icons.py. -->\n'
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24">\n'
            f'    <path android:fillColor="#FF000000" android:pathData="{path}" />\n</vector>\n'
        )
    # The app and the widget compile this catalog, so the widget shows the same icons.
    files[IOS / "Shared/Glyphs.xcassets/Contents.json"] = json.dumps({"info": {"author": "xcode", "version": 1}}, indent=2) + "\n"
    license_text = (DESKTOP / "public/licenses/Remix-LICENSE.txt").read_text()
    files[IOS / "Resources/Remix-LICENSE.txt"] = license_text
    files[ROOT / "src/dmsg_android/app/src/main/assets/licenses/Remix-LICENSE.txt"] = license_text
    stale = []
    for path, expected in files.items():
        if args.check:
            if not path.exists() or path.read_text() != expected:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(expected)
    expected_assets = {path.parent.name for path in files if path.parent.suffix == ".symbolset"}
    for path in [*(IOS / "Resources/Assets.xcassets").glob("Glyph-*.*set"), *(IOS / "Shared/Glyphs.xcassets").glob("*.*set")]:
        if path.name not in expected_assets:
            stale.append(f"Unused asset: {path.relative_to(ROOT)}")
    for path in (ANDROID / "res/drawable").glob("glyph_*.xml"):
        if path not in files:
            stale.append(f"Unused drawable: {path.relative_to(ROOT)}")
    if stale:
        raise SystemExit("Icons out of sync; run scripts/ios/sync-icons.py:\n" + "\n".join(stale))
    print(f"{len(names)} business icons for iOS and Android {'verified against' if args.check else 'generated from'} desktop paths.")


if __name__ == "__main__":
    main()
