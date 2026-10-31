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

    def write_archive(self):
        archive = self.output / "dMsg.xcarchive"
        bundles = [("dMsg.app", "net.dmsg.ios"),
                   ("dMsg.app/PlugIns/Share.appex", "net.dmsg.ios.share")]
        for path, bundle in bundles:
            directory = archive / "Products/Applications" / path
            directory.mkdir(parents=True)
            (directory / "Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": bundle, "CFBundleShortVersionString": "0.1.0",
                "CFBundleVersion": "5"}))
        (archive / "Info.plist").write_bytes(plistlib.dumps({"ApplicationProperties": {
            "ApplicationPath": "Applications/dMsg.app", "CFBundleIdentifier": "net.dmsg.ios",
            "CFBundleShortVersionString": "0.1.0", "CFBundleVersion": "5", "Team": "S9Z2WAMAV8"}}))
        return archive

    def package(self, dry_run=False, auth_args=()):
        with patch.object(testflight, "ROOT", self.root), contextlib.redirect_stdout(io.StringIO()):
            with patch.object(testflight.subprocess, "check_output", return_value=json.dumps(self.spec)):
                testflight.package(dry_run, auth_args)

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
        key = self.root / "Auth Key.p8"
        key.write_text("private key content must not be copied to the output")
        auth_args = testflight.authentication_args(key, "KEY123", "issuer")

        def build(command, log, **kwargs):
            calls.append((command, kwargs))
            if "-exportArchive" in command:
                self.write_ipa(Path(command[command.index("-exportPath") + 1]))

        with patch.object(testflight, "run_logged", side_effect=build):
            self.package(auth_args=auth_args)
        self.assertEqual(len(calls), 2)
        for command, _ in calls:
            self.assertEqual(command[-6:], ["-authenticationKeyPath", str(key.resolve()),
                "-authenticationKeyID", "KEY123", "-authenticationKeyIssuerID", "issuer"])
        self.assertIn(str(self.output / "dMsg.xcarchive"), calls[0][0])
        self.assertEqual(calls[1][1]["env"]["PATH"], "/usr/bin:/bin:/usr/sbin:/sbin")
        options = plistlib.loads((self.output / "ExportOptions.plist").read_bytes())
        self.assertEqual(options["destination"], "export")
        self.assertEqual(options["method"], "app-store-connect")
        self.assertFalse(options["manageAppVersionAndBuildNumber"])
        self.assertTrue((self.output / "Distribution/dMsg.ipa.sha256").is_file())
        self.assertIn('CURRENT_PROJECT_VERSION: "5" # app', self.project.read_text())

    def test_incomplete_or_missing_key_fails_before_reserving_build(self):
        for args, error in ((["--authentication-key-id", "KEY123"], ValueError),
                            (["--authentication-key-path", str(self.root / "missing.p8"),
                              "--authentication-key-id", "KEY123",
                              "--authentication-key-issuer-id", "issuer"], FileNotFoundError)):
            with self.subTest(args=args), patch.object(testflight, "ROOT", self.root), \
                    patch.object(testflight.sys, "argv", ["testflight.py", *args]), \
                    patch.object(testflight.subprocess, "check_output") as command:
                with self.assertRaises(error):
                    testflight.main()
                command.assert_not_called()
                self.assertEqual(self.project.read_text(), self.source)
                self.assertFalse(self.output.exists())

    def test_xcode_account_authentication_needs_no_key_options(self):
        self.assertEqual(testflight.authentication_args(None, None, None), [])

    def test_export_failure_explains_signing_and_retains_build(self):
        def fail_export(command, log, **kwargs):
            if "-exportArchive" in command:
                log.write_text('error: exportArchive No Accounts\n'
                               'error: exportArchive No signing certificate "iOS Distribution" found\n')
                raise RuntimeError(f"Command failed. See {log}")
            self.write_archive()

        errors = io.StringIO()
        with patch.object(testflight, "run_logged", side_effect=fail_export), \
                contextlib.redirect_stderr(errors):
            with self.assertRaisesRegex(RuntimeError, "--export-archive") as failure:
                self.package()
        self.assertIn("--authentication-key-*", errors.getvalue())
        self.assertIn("S9Z2WAMAV8", errors.getvalue())
        self.assertIn("export.log", str(failure.exception))
        self.assertTrue((self.output / "dMsg.xcarchive/Info.plist").exists())
        self.assertIn('CURRENT_PROJECT_VERSION: "5" # app', self.project.read_text())
        self.assertFalse(list(self.output.rglob("*.ipa.sha256")))

    def test_retry_uses_archive_without_build_tools_or_changing_old_output(self):
        archive = self.write_archive()
        self.project.unlink()  # An old archive does not depend on the current source tree.
        old_log = self.output / "export.log"
        old_log.write_text("previous failure")
        old_ipa = self.write_ipa(self.output / "Distribution")
        old_bytes = old_ipa.read_bytes()
        calls = []

        def export(command, log, **kwargs):
            self.assertIn("-exportArchive", command)
            calls.append(command)
            log.write_text("export succeeded")
            self.write_ipa(Path(command[command.index("-exportPath") + 1]))

        with patch.object(testflight, "ROOT", self.root), \
                patch.object(testflight.sys, "argv", ["testflight.py", "--export-archive", str(archive)]), \
                patch.object(testflight.shutil, "which", side_effect=lambda tool: tool if tool == "xcodebuild" else None), \
                patch.object(testflight.subprocess, "check_output") as check_output, \
                patch.object(testflight.subprocess, "run") as run, \
                patch.object(testflight, "run_logged", side_effect=export), \
                contextlib.redirect_stdout(io.StringIO()):
            testflight.main()
            testflight.main()
            check_output.assert_not_called()
            run.assert_not_called()
        self.assertEqual(len(calls), 2)
        self.assertNotEqual(calls[0], calls[1])
        self.assertEqual(old_log.read_text(), "previous failure")
        self.assertEqual(old_ipa.read_bytes(), old_bytes)
        self.assertFalse(self.project.exists())
        retries = list(self.output.glob("Export-*"))
        self.assertEqual(len(retries), 2)
        for output in retries:
            self.assertTrue((output / "Distribution/dMsg.ipa.sha256").is_file())
            options = plistlib.loads((output / "ExportOptions.plist").read_bytes())
            self.assertEqual(options["teamID"], "S9Z2WAMAV8")
            self.assertEqual(options["destination"], "export")
            self.assertFalse(options["manageAppVersionAndBuildNumber"])

    def test_retry_dry_run_does_not_export_or_create_output(self):
        archive = self.write_archive()
        before = set(self.output.rglob("*"))
        with patch.object(testflight, "run_logged") as run, contextlib.redirect_stdout(io.StringIO()):
            testflight.retry_export(archive, True)
            run.assert_not_called()
        self.assertEqual(set(self.output.rglob("*")), before)
        self.assertEqual(self.project.read_text(), self.source)

    def test_inconsistent_archive_fails_before_export(self):
        archive = self.write_archive()
        extension = archive / "Products/Applications/dMsg.app/PlugIns/Share.appex/Info.plist"
        original = plistlib.loads(extension.read_bytes())
        for key, value in (("CFBundleVersion", "4"), ("CFBundleIdentifier", "net.dmsg.ios")):
            with self.subTest(key=key), patch.object(testflight, "run_logged") as run:
                extension.write_bytes(plistlib.dumps(dict(original, **{key: value})))
                with self.assertRaises(ValueError):
                    testflight.retry_export(archive, False)
                run.assert_not_called()
                self.assertFalse(list(self.output.glob("Export-*")))

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
