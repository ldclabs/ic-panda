#!/usr/bin/env python3
"""Increment the iOS build number, archive Release, and export a TestFlight IPA."""
import argparse
from collections import Counter
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
BUILD_SETTING = re.compile(
    r"^([ \t]*CURRENT_PROJECT_VERSION:[ \t]*)([\"']?)([0-9]+)\2([ \t]*(?:#[^\n]*)?)$",
    re.MULTILINE,
)


def next_build(source, spec):
    """Use XcodeGen's parsed settings; edit only the existing build-number scalars."""
    targets = {
        name: target["settings"]["base"]
        for name, target in spec["targets"].items()
        if target["type"] in ("application", "app-extension")
    }
    if "dMsg" not in targets:
        raise ValueError("project.yml must contain the dMsg application")
    builds = [str(settings.get("CURRENT_PROJECT_VERSION", "")) for settings in targets.values()]
    if any(not re.fullmatch(r"[0-9]+", build) for build in builds):
        raise ValueError("Each app/extension needs an integer CURRENT_PROJECT_VERSION in settings.base")
    matches = list(BUILD_SETTING.finditer(source))
    if Counter(match[3] for match in matches) != Counter(builds):
        raise ValueError("Unexpected build-number layout/overrides in project.yml; no files changed")
    versions = {str(settings.get("MARKETING_VERSION", "")) for settings in targets.values()}
    if len(versions) != 1 or not re.fullmatch(r"[0-9]+(?:\.[0-9]+){0,2}", next(iter(versions))):
        raise ValueError("The app and all extensions must have the same numeric MARKETING_VERSION")
    team = str(spec["settings"]["base"].get("DEVELOPMENT_TEAM", ""))
    if not re.fullmatch(r"[A-Z0-9]{10}", team):
        raise ValueError("Set DEVELOPMENT_TEAM in project.yml before exporting")
    bundle_ids = {settings["PRODUCT_BUNDLE_IDENTIFIER"] for settings in targets.values()}
    if len(bundle_ids) != len(targets):
        raise ValueError("App/extension bundle identifiers must be unique")
    build = str(max(map(int, builds)) + 1)
    updated = BUILD_SETTING.sub(lambda m: f"{m[1]}{m[2]}{build}{m[2]}{m[4]}", source)
    return updated, versions.pop(), build, team, bundle_ids


def run_logged(command, log_path, *, env=None):
    print(f"\n$ {shlex.join(map(str, command))}\nLog: {log_path}", flush=True)
    with log_path.open("w") as log:
        with subprocess.Popen(command, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, text=True, errors="replace") as process:
            for line in process.stdout:
                log.write(line)
                log.flush()
                print(line, end="", flush=True)
            if process.wait() != 0:
                raise RuntimeError(f"Command failed. See {log_path}")


def verify_ipa(ipa, version, build, bundle_ids):
    with zipfile.ZipFile(ipa) as package:
        info_paths = [name for name in package.namelist() if re.fullmatch(
            r"Payload/[^/]+\.app/(?:PlugIns/[^/]+\.appex/)?Info\.plist", name
        )]
        actual_ids = []
        for name in info_paths:
            info = plistlib.loads(package.read(name))
            if (info.get("CFBundleShortVersionString"), info.get("CFBundleVersion")) != (version, build):
                raise ValueError(f"IPA version/build mismatch: {name}")
            actual_ids.append(info.get("CFBundleIdentifier"))
        if Counter(actual_ids) != Counter(bundle_ids):
            raise ValueError("IPA is missing an expected app/extension or contains an unexpected bundle")


def authentication_args(key_path, key_id, issuer_id):
    if not any((key_path, key_id, issuer_id)):
        return []
    if not all((key_path, key_id, issuer_id)):
        raise ValueError("Pass --authentication-key-path, --authentication-key-id and "
                         "--authentication-key-issuer-id together (App Store Connect team API key)")
    key_path = Path(key_path).expanduser().resolve()
    # Check readability before reserving a build number; never copy or print the key.
    with key_path.open("rb"):
        pass
    return ["-authenticationKeyPath", str(key_path), "-authenticationKeyID", key_id,
            "-authenticationKeyIssuerID", issuer_id]


def archive_metadata(archive):
    """Retry from the archived bundles, even if project.yml has since changed."""
    properties = plistlib.loads((archive / "Info.plist").read_bytes())["ApplicationProperties"]
    version, build, team = (properties[key] for key in
                            ("CFBundleShortVersionString", "CFBundleVersion", "Team"))
    if (not re.fullmatch(r"[0-9]+(?:\.[0-9]+){0,2}", version)
            or not re.fullmatch(r"[0-9]+", build) or not re.fullmatch(r"[A-Z0-9]{10}", team)):
        raise ValueError("Archive must contain a numeric version/build and a development team")
    apps = list((archive / "Products/Applications").glob("*.app"))
    if len(apps) != 1:
        raise ValueError("Archive must contain exactly one application")
    app = apps[0]
    if properties["ApplicationPath"] != str(app.relative_to(archive / "Products")):
        raise ValueError("Archive application path does not match its metadata")
    bundle_ids = []
    for bundle in [app, *sorted(app.glob("PlugIns/*.appex"))]:
        info = plistlib.loads((bundle / "Info.plist").read_bytes())
        if (info.get("CFBundleShortVersionString"), info.get("CFBundleVersion")) != (version, build):
            raise ValueError(f"Archive version/build mismatch: {bundle}")
        bundle_id = info.get("CFBundleIdentifier")
        if not bundle_id or bundle_id in bundle_ids:
            raise ValueError("Archive bundle identifiers must be present and unique")
        bundle_ids.append(bundle_id)
    if bundle_ids[0] != properties["CFBundleIdentifier"]:
        raise ValueError("Archive application identifier does not match its metadata")
    return version, build, team, set(bundle_ids)


def export_archive(archive, output, version, build, team, bundle_ids, auth_args=()):
    export_options = output / "ExportOptions.plist"
    export_options.write_bytes(plistlib.dumps({
        "method": "app-store-connect",
        "destination": "export",
        "teamID": team,
        "signingStyle": "automatic",
        "manageAppVersionAndBuildNumber": False,
    }))
    # Apple's export invokes system rsync. A Homebrew rsync on PATH breaks its flags.
    export_env = dict(os.environ, PATH="/usr/bin:/bin:/usr/sbin:/sbin")
    log = output / "export.log"
    try:
        run_logged(["/usr/bin/xcodebuild", "-exportArchive", "-archivePath", str(archive),
                    "-exportPath", str(output / "Distribution"),
                    "-exportOptionsPlist", str(export_options), "-allowProvisioningUpdates",
                    *auth_args], log, env=export_env)
    except RuntimeError as error:
        details = log.read_text(errors="replace") if log.exists() else ""
        if "No Accounts" in details or "No signing certificate" in details:
            print(f"\nDistribution signing is unavailable for team {team}. "
                  "A development-signed archive is not sufficient for TestFlight.\n"
                  "In the selected Xcode's Settings > Accounts, sign in to this team and check "
                  "App Store Connect / cloud-managed distribution certificate access, or pass "
                  "the three --authentication-key-* options for a team API key.\n"
                  "Local signing requires an Apple Distribution certificate WITH its private key; "
                  "Apple Development and Developer ID Application cannot replace it.", file=sys.stderr)
        retry = ["python3", str(ROOT / "scripts/ios/testflight.py"), "--export-archive", str(archive)]
        raise RuntimeError(f"{error}\nArchive retained. Retry without rebuilding or incrementing "
                           f"the build number:\n{shlex.join(retry)}\n"
                           "Add the same --authentication-key-* options if using an API key.") from error
    ipas = list((output / "Distribution").glob("*.ipa"))
    if len(ipas) != 1:
        raise RuntimeError("Export did not produce exactly one IPA")
    ipa = ipas[0]
    verify_ipa(ipa, version, build, bundle_ids)
    digest = hashlib.sha256()
    with ipa.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    checksum = ipa.with_suffix(".ipa.sha256")
    checksum.write_text(f"{digest.hexdigest()}  {ipa.name}\n")
    print(f"\nTestFlight IPA ready: {version} ({build})\nIPA: {ipa}\nSHA-256: {checksum}")
    print("Add this IPA to Transporter to upload. No upload or review submission was performed.")


def package(dry_run, auth_args=()):
    project = ROOT / "src/dmsg_ios/project.yml"
    source = project.read_text()
    spec = json.loads(subprocess.check_output(
        ["xcodegen", "dump", "--spec", str(project), "--type", "json"], cwd=ROOT, text=True
    ))
    updated, version, build, team, bundle_ids = next_build(source, spec)
    output = ROOT / f"src/dmsg_ios/.build/TestFlight-{version}-{build}"
    print(f"Next build: {version} ({build}); {len(bundle_ids)} bundles; team {team}", flush=True)
    print(f"Output: {output}", flush=True)
    if dry_run:
        print("Dry run: no files changed; no archive, export or upload performed.")
        return
    output.mkdir(parents=True, exist_ok=False)
    archive = output / "dMsg.xcarchive"
    # Keep existing edits and formatting, and never reset the number after a failure.
    if project.read_text() != source:
        raise RuntimeError("project.yml changed during preflight; rerun with the current settings")
    project.write_text(updated)
    print(f"Updated project.yml to build {build}. This number is retained if packaging fails.", flush=True)
    run_logged(["bash", "scripts/ios/archive.sh", "--archive-path", str(archive),
                "-allowProvisioningUpdates", *auth_args], output / "archive.log")
    export_archive(archive, output, version, build, team, bundle_ids, auth_args)


def retry_export(archive, dry_run, auth_args=()):
    archive = archive.expanduser().resolve()
    version, build, team, bundle_ids = archive_metadata(archive)
    print(f"Existing archive: {version} ({build}); {len(bundle_ids)} bundles; team {team}\n"
          f"Archive: {archive}", flush=True)
    if dry_run:
        print("Dry run: no files changed; no archive, export or upload performed.")
        return
    # Keep every earlier attempt's logs and IPA, including a partially failed export.
    output = Path(tempfile.mkdtemp(prefix="Export-", dir=archive.parent))
    print(f"Output: {output}", flush=True)
    export_archive(archive, output, version, build, team, bundle_ids, auth_args)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true",
                        help="preview the next build or inspect an existing archive without changing files")
    parser.add_argument("--export-archive", type=Path, metavar="XCARCHIVE",
                        help="export an existing archive without rebuilding or changing project.yml")
    parser.add_argument("--authentication-key-path", metavar="P8",
                        help="App Store Connect team API private key file (kept outside the repository)")
    parser.add_argument("--authentication-key-id", metavar="KEY_ID")
    parser.add_argument("--authentication-key-issuer-id", metavar="ISSUER_ID")
    args = parser.parse_args()
    auth_args = authentication_args(args.authentication_key_path, args.authentication_key_id,
                                    args.authentication_key_issuer_id)
    required_tools = ("xcodebuild",) if args.export_archive else ("xcodegen", "xcodebuild", "cargo", "rustup")
    for tool in required_tools:
        if not shutil.which(tool):
            parser.error(f"Missing {tool}; see src/dmsg_ios/README.md")
    if args.dry_run:
        if args.export_archive:
            retry_export(args.export_archive, True, auth_args)
        else:
            package(True, auth_args)
        return
    if not args.export_archive:
        subprocess.run(["xcrun", "--sdk", "iphoneos", "--show-sdk-path"], check=True, stdout=subprocess.DEVNULL)
        installed = subprocess.check_output(["rustup", "target", "list", "--installed"], text=True).splitlines()
        if not {"aarch64-apple-ios", "aarch64-apple-ios-sim"}.issubset(installed):
            parser.error("Run: rustup target add aarch64-apple-ios aarch64-apple-ios-sim")
    build_dir = ROOT / "src/dmsg_ios/.build"
    build_dir.mkdir(parents=True, exist_ok=True)
    with (build_dir / ".testflight.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("Another TestFlight packaging script is running in this checkout") from None
        if args.export_archive:
            retry_export(args.export_archive, False, auth_args)
        else:
            package(False, auth_args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print(f"TestFlight packaging failed: {error}", file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print("\nPackaging interrupted. The reserved build number and logs are retained.", file=sys.stderr)
        sys.exit(130)
