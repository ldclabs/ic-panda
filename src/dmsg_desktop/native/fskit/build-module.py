#!/usr/bin/env python3
"""Build the FSKit module. --provision opts in to updating Apple development profiles."""
import argparse
import pathlib
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", required=True)
parser.add_argument("--team", required=True)
parser.add_argument("--identity", default="Apple Development")
parser.add_argument("--profile", help="Installed provisioning profile name or UUID for manual signing")
parser.add_argument("--configuration", choices=["Debug", "Release"], default="Debug")
parser.add_argument("--provision", action="store_true")
args = parser.parse_args()

src = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(args.output).resolve()
project = out / "project"
project.mkdir(parents=True, exist_ok=True)
for source in src.iterdir():
    if source.suffix in {".swift", ".plist", ".entitlements", ".yml"}:
        shutil.copyfile(source, project / source.name)
subprocess.run(["xcodegen", "generate", "--spec", str(project / "project.yml"),
                "--project", str(project)], check=True)

command = [
    "xcodebuild", "-project", str(project / "DmsgSecretsProbe.xcodeproj"),
    "-scheme", "SecretsFS", "-configuration", args.configuration,
    "-destination", "generic/platform=macOS", "-derivedDataPath", str(out / "derived"),
    "CODE_SIGN_STYLE=" + ("Manual" if args.profile else "Automatic"),
    "CODE_SIGN_IDENTITY=" + args.identity, "DEVELOPMENT_TEAM=" + args.team,
]
if args.profile:
    command.append("PROVISIONING_PROFILE_SPECIFIER=" + args.profile)
if args.configuration == "Release" and args.profile:
    command.append("OTHER_CODE_SIGN_FLAGS=--timestamp")
if args.provision:
    command.append("-allowProvisioningUpdates")
subprocess.run([*command, "build"], check=True)

extension = out / "derived/Build/Products" / args.configuration / "SecretsFS.appex"
if not (extension / "Contents/embedded.provisionprofile").is_file():
    raise SystemExit("Missing FSKit Module provisioning profile")
subprocess.run(["codesign", "--verify", "--strict", str(extension)], check=True)
print(extension)
