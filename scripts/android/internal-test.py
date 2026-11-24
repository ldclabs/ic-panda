#!/usr/bin/env python3
"""Build and verify the signed Google Play internal-test bundle locally."""
import ctypes
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
ANDROID = ROOT / "src/dmsg_android"
BUILD = (ANDROID / "app/build").resolve()
OUTPUT = ROOT / "misc/dmsg-play-internal"


def build_environment():
    env = os.environ.copy()
    if sys.platform == "darwin":
        env.setdefault("ANDROID_HOME", str(Path.home() / "Library/Android/sdk"))
        if "JAVA_HOME" not in env and shutil.which("brew"):
            prefix = subprocess.check_output(["brew", "--prefix", "openjdk@21"], text=True).strip()
            env["JAVA_HOME"] = prefix
    if (BUILD.parent / "tmp").is_dir():
        env["TMPDIR"] = str(BUILD.parent / "tmp")
    return env


def keychain_password():
    security = ctypes.CDLL("/System/Library/Frameworks/Security.framework/Security")
    service = b"net.dmsg.android.play-upload"
    account = b"dmsg-upload"
    size, data = ctypes.c_uint32(), ctypes.c_void_p()
    status = security.SecKeychainFindGenericPassword(
        None, len(service), service, len(account), account, ctypes.byref(size), ctypes.byref(data), None)
    if status:
        sys.exit(f"Cannot read the dMsg upload password from Keychain (OSStatus {status}).")
    try:
        return ctypes.string_at(data, size.value).decode()
    finally:
        security.SecKeychainItemFreeContent(None, data)


def signing_environment(env):
    env = env.copy()
    if sys.platform == "darwin" and not env.get("DMSG_ANDROID_KEYSTORE"):
        store = Path.home() / "Library/Application Support/dMsg/PlaySigning/net.dmsg.android-upload.p12"
        if not store.is_file():
            sys.exit("Set DMSG_ANDROID_KEYSTORE to the Google Play upload keystore.")
        password = keychain_password()
        env.update(DMSG_ANDROID_KEYSTORE=str(store), DMSG_ANDROID_KEY_ALIAS="dmsg-upload",
                   DMSG_ANDROID_STORE_PASSWORD=password, DMSG_ANDROID_KEY_PASSWORD=password)
    for name in ("KEYSTORE", "KEY_ALIAS", "STORE_PASSWORD", "KEY_PASSWORD"):
        if not env.get("DMSG_ANDROID_" + name):
            sys.exit("Missing DMSG_ANDROID_" + name)
    env["DMSG_ANDROID_KEYSTORE"] = str(Path(env["DMSG_ANDROID_KEYSTORE"]).resolve(strict=True))
    return env


def main():
    env = build_environment()
    config = env.get("DMSG_ANDROID_CLIENT_FILE")
    if config:
        path = Path(config).resolve(strict=True)
        client = json.loads(path.read_text())
        if client["package_name"] != "net.dmsg.android" or client["environment"] != "production":
            sys.exit("Expected the net.dmsg.android production Firebase client.")
        env["DMSG_ANDROID_CLIENT_FILE"] = str(path)
    elif not (ANDROID / "app/google-services.json").is_file():
        sys.exit("Supply app/google-services.json or set DMSG_ANDROID_CLIENT_FILE.")

    env = signing_environment(env)
    subprocess.run([str(ANDROID / "gradlew"), "--console=plain", "--dependency-verification=strict",
                    "--max-workers=3", "-Pdmsg.abis=arm64-v8a,x86_64", ":app:bundleProductionGmsRelease"],
                   cwd=ANDROID, env=env, check=True)
    variant = "productionGmsRelease"
    bundle = BUILD / "outputs/bundle" / variant / "app-production-gms-release.aab"
    metadata = BUILD / "intermediates/packaged_manifests" / variant / "processProductionGmsReleaseManifestForPackage/output-metadata.json"
    manifest = json.loads(metadata.read_text())
    if manifest["applicationId"] != "net.dmsg.android" or manifest["variantName"] != variant:
        sys.exit("Unexpected package or variant in release metadata.")
    version, = manifest["elements"]
    name = f"dmsg-{version['versionName']}-{version['versionCode']}-gms"
    jarsigner = str(Path(env["JAVA_HOME"]) / "bin/jarsigner") if env.get("JAVA_HOME") else "jarsigner"
    with zipfile.ZipFile(bundle) as archive:
        if not any(p.startswith("META-INF/") and p.endswith((".RSA", ".DSA", ".EC")) for p in archive.namelist()):
            sys.exit("Gradle produced an unsigned bundle.")
    subprocess.run([jarsigner, "-verify", "-strict", "-keystore", env["DMSG_ANDROID_KEYSTORE"],
                    "-storepass:env", "DMSG_ANDROID_STORE_PASSWORD", str(bundle), env["DMSG_ANDROID_KEY_ALIAS"]],
                   env=env, check=True)

    OUTPUT.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=OUTPUT) as tmp:
        delivery = Path(tmp) / (name + ".aab")
        report = delivery.with_suffix(".verification.json")
        shutil.copyfile(bundle, delivery)
        subprocess.run([sys.executable, str(ROOT / "scripts/android/verify-package.py"),
                        str(delivery), "--output", str(report)], check=True)
        delivery.replace(OUTPUT / delivery.name)
        report.replace(OUTPUT / report.name)
    print(f"Upload to Google Play: {OUTPUT / delivery.name}")


if __name__ == "__main__":
    main()
