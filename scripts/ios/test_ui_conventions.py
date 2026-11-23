"""Keep iOS pages on the shared palette, type scale and shape tokens (src/dmsg_ios/DESIGN.md)."""
import json
import pathlib
import re
import unittest

IOS = pathlib.Path(__file__).resolve().parents[2] / "src/dmsg_ios"
SOURCES = ["App", "Features", "Platform", "Shared", "Intents", "Extensions"]
# Token definitions, and the DEBUG-only touch overlay drawn for App Store previews.
EXEMPT = {"Features/Theme.swift", "Shared/NativeTheme.swift", "App/PreviewTouches.swift"}
PALETTE = IOS / "Shared/Palette.xcassets"

RULES = {
    "colour literal": re.compile(r"\b(?:UI)?Color\((?:red|white|hue|displayP3Red):|#colorLiteral|Color\(0x"),
    "tinted wash (use Theme.fill)": re.compile(r"forest(?:Fill)?\)?\.opacity\("),
    "fixed font size": re.compile(r"\.font\(\.system\(size:\s*\d"),
    "numeric corner radius (use Theme.radius*)": re.compile(r"cornerRadius:\s*\d"),
    "caption text (use footnote; caption2 is for badges)": re.compile(r"\.caption\b(?!2)"),
}


def sources():
    for folder in SOURCES:
        for path in sorted((IOS / folder).rglob("*.swift")):
            relative = path.relative_to(IOS).as_posix()
            if relative not in EXEMPT:
                yield relative, path.read_text()


def palette():
    colours = {}
    for folder in PALETTE.glob("*.colorset"):
        entries = json.loads((folder / "Contents.json").read_text())["colors"]
        for entry in entries:
            dark = any(item.get("value") == "dark" for item in entry.get("appearances", []))
            components = entry["color"]["components"]
            colours[(folder.stem, "dark" if dark else "light")] = tuple(
                int(components[channel], 16) / 255 if components[channel].startswith("0x") else float(components[channel])
                for channel in ("red", "green", "blue"))
    return colours


def contrast(first, second):
    def luminance(rgb):
        linear = [value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4 for value in rgb]
        return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    high, low = sorted((luminance(first), luminance(second)), reverse=True)
    return (high + 0.05) / (low + 0.05)


class UIConventionTests(unittest.TestCase):
    def test_pages_use_tokens(self):
        failures = []
        for relative, text in sources():
            for number, line in enumerate(text.splitlines(), 1):
                if line.lstrip().startswith("//"):
                    continue
                for name, pattern in RULES.items():
                    if pattern.search(line):
                        failures.append(f"{relative}:{number}: {name}")
        self.assertEqual(failures, [], "\n" + "\n".join(failures))

    def test_named_colours_exist(self):
        names = {folder.stem for folder in PALETTE.glob("*.colorset")}
        used = set()
        for _, text in sources():
            used.update(re.findall(r'\b(?:Color\(|UIColor\(named:\s*)"(\w+)"', text))
        used.update(re.findall(r'Color\("(\w+)"\)', (IOS / "Features/Theme.swift").read_text()))
        used.update(re.findall(r'named: "(\w+)"', (IOS / "Shared/NativeTheme.swift").read_text()))
        self.assertEqual(used - names, set())
        self.assertIn("Forest", names, "ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME needs it")

    def test_cards_stand_apart_from_their_pages(self):
        colours = palette()
        # System rows are white in light mode and #1C1C1E in dark mode on grouped pages.
        system_dark_row = (0x1C / 255, 0x1C / 255, 0x1E / 255)
        pairs = {
            "light grouped page / card": (colours[("PaperGrouped", "light")], colours[("Card", "light")]),
            "dark grouped page / system row": (colours[("PaperGrouped", "dark")], system_dark_row),
            "dark plain page / card": (colours[("Paper", "dark")], colours[("Card", "dark")]),
        }
        for name, (page, card) in pairs.items():
            self.assertGreaterEqual(contrast(page, card), 1.12, name)
        for appearance in ("light", "dark"):
            self.assertGreaterEqual(contrast(colours[("Card", appearance)], colours[("Fill", appearance)]), 1.12, appearance)
            # White labels on filled actions, own bubbles and approval counts.
            self.assertGreaterEqual(contrast(colours[("ForestFill", appearance)], (1, 1, 1)), 4.5, appearance)
            self.assertGreaterEqual(contrast(colours[("BadgeRed", appearance)], (1, 1, 1)), 4.5, appearance)
            for tone in ("AvatarForest", "AvatarSage", "AvatarAmber"):
                self.assertGreaterEqual(contrast(colours[(tone, appearance)], colours[(tone + "Ink", appearance)]), 4.5, tone)


if __name__ == "__main__":
    unittest.main()
