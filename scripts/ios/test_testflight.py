"""Packaging contracts without invoking signing, Rust builds, or Apple services."""
import contextlib
import copy
import io
import json
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import testflight


class TestFlightTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="dmsg packaging ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.project = self.root / "src/dmsg_ios/project.yml"
        self.project.parent.mkdir(parents=True)
        self.source = ('# preserve this draft\nCURRENT_PROJECT_VERSION: "4" # app\n'
                       "  CURRENT_PROJECT_VERSION: '3' # extension\nDMSG_IN_APP_PURCHASES_ENABLED: NO\n")
        self.project.write_text(self.source)
        self.spec = {"settings": {"base": {"DEVELOPMENT_TEAM": "S9Z2WAMAV8"}}, "targets": {
            "dMsg": {"type": "application", "settings": {"base": {
                "CURRENT_PROJECT_VERSION": "4", "MARKETING_VERSION": "0.1.0",
                "PRODUCT_BUNDLE_IDENTIFIER": "net.dmsg.ios"}}},
            "Share": {"type": "app-extension", "settings": {"base": {
                "CURRENT_PROJECT_VERSION": "3", "MARKETING_VERSION": "0.1.0",
                "PRODUCT_BUNDLE_IDENTIFIER": "net.dmsg.ios.share"}}},
        }}
        self.output = self.project.parent / ".build/TestFlight-0.1.0-5"

    def write_ipa(self, directory, extension_build="5", include_extension=True):
        directory.mkdir(parents=True, exist_ok=True)
        ipa = directory / "dMsg.ipa"
        with zipfile.ZipFile(ipa, "w") as package:
            bundles = [("Payload/dMsg.app/Info.plist", "net.dmsg.ios", "5")]
            if include_extension:
                bundles.append(("Payload/dMsg.app/PlugIns/Share.appex/Info.plist",
                                "net.dmsg.ios.share", extension_build))
            for path, bundle, build in bundles:
                package.writestr(path, plistlib.dumps({"CFBundleIdentifier": bundle,
                    "CFBundleVersion": build, "CFBundleShortVersionString": "0.1.0"}))
        return ipa

    def package(self, dry_run=False):
        with patch.object(testflight, "ROOT", self.root), contextlib.redirect_stdout(io.StringIO()):
            with patch.object(testflight.subprocess, "check_output", return_value=json.dumps(self.spec)):
                testflight.package(dry_run)

    def test_increment_mixed_builds_preserves_draft_and_format(self):
        updated, version, build, team, bundles = testflight.next_build(self.source, self.spec)
        self.assertEqual(updated, self.source.replace('"4"', '"5"').replace("'3'", "'5'"))
        self.assertEqual((version, build, team), ("0.1.0", "5", "S9Z2WAMAV8"))
        self.assertEqual(bundles, {"net.dmsg.ios", "net.dmsg.ios.share"})

    def test_invalid_settings_fail_before_rewrite(self):
        for key, value in (("CURRENT_PROJECT_VERSION", "4.1"), ("MARKETING_VERSION", "0.2.0")):
            with self.subTest(key=key):
                spec = copy.deepcopy(self.spec)
                spec["targets"]["Share"]["settings"]["base"][key] = value
                with self.assertRaises(ValueError):
                    testflight.next_build(self.source, spec)
        with self.assertRaises(ValueError):
            testflight.next_build(self.source + "CURRENT_PROJECT_VERSION: 99\n", self.spec)

    def test_dry_run_does_not_change_source_or_create_output(self):
        self.package(dry_run=True)
        self.assertEqual(self.project.read_text(), self.source)
        self.assertFalse(self.output.parent.exists())

    def test_success_exports_without_upload_and_checks_bundles(self):
        calls = []

        def build(command, log, **kwargs):
            calls.append((command, kwargs))
            if "-exportArchive" in command:
                self.write_ipa(Path(command[command.index("-exportPath") + 1]))

        with patch.object(testflight, "run_logged", side_effect=build):
            self.package()
        self.assertEqual(len(calls), 2)
        self.assertIn(str(self.output / "dMsg.xcarchive"), calls[0][0])
        self.assertEqual(calls[1][1]["env"]["PATH"], "/usr/bin:/bin:/usr/sbin:/sbin")
        options = plistlib.loads((self.output / "ExportOptions.plist").read_bytes())
        self.assertEqual(options["destination"], "export")
        self.assertEqual(options["method"], "app-store-connect")
        self.assertFalse(options["manageAppVersionAndBuildNumber"])
        self.assertTrue((self.output / "Distribution/dMsg.ipa.sha256").is_file())
        self.assertIn('CURRENT_PROJECT_VERSION: "5" # app', self.project.read_text())

    def test_failed_archive_keeps_number_and_stops_export(self):
        with patch.object(testflight, "run_logged", side_effect=RuntimeError("archive failed")) as build:
            with self.assertRaisesRegex(RuntimeError, "archive failed"):
                self.package()
        self.assertEqual(build.call_count, 1)
        self.assertIn("CURRENT_PROJECT_VERSION: '5' # extension", self.project.read_text())
        self.assertFalse((self.output / "Distribution").exists())

    def test_existing_output_is_not_overwritten_or_number_consumed(self):
        self.output.mkdir(parents=True)
        marker = self.output / "archive.log"
        marker.write_text("existing build")
        with self.assertRaises(FileExistsError):
            self.package()
        self.assertEqual(self.project.read_text(), self.source)
        self.assertEqual(marker.read_text(), "existing build")

    def test_stale_or_missing_extension_is_not_reported_as_success(self):
        for kwargs in ({"extension_build": "3"}, {"include_extension": False}):
            with self.subTest(kwargs=kwargs):
                ipa = self.write_ipa(self.output, **kwargs)
                with self.assertRaises(ValueError):
                    testflight.verify_ipa(ipa, "0.1.0", "5", {"net.dmsg.ios", "net.dmsg.ios.share"})


if __name__ == "__main__":
    unittest.main()
