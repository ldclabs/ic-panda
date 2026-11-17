#!/usr/bin/env python3
"""Black-box ordinary dMsg smoke on an explicitly disposable emulator.

Run after installing the reviewed APK or AAB splits. No test code is injected into
the minified app, no account is created, and no browser credentials are entered.
"""
import argparse
import json
import re
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--disposable-emulator", action="store_true", required=True)
    parser.add_argument("--login-probe", action="store_true", help="Opt in to read-only public deployment preflight and browser launch")
    args = parser.parse_args()
    if not args.serial.startswith("emulator-"):
        parser.error("A dedicated emulator is required")
    package = "net.dmsg.android"
    component = package + "/net.dmsg.android.MainActivity"
    args.output.mkdir(parents=True, exist_ok=True)

    def adb(*command):
        return subprocess.check_output([args.adb, "-s", args.serial, *command], text=True, timeout=45)

    if adb("shell", "getprop", "ro.kernel.qemu").strip() != "1":
        parser.error("The selected target is not an emulator")

    def tree(name):
        remote = "/sdcard/Download/dmsg-release-smoke.xml"
        adb("shell", "uiautomator", "dump", remote)
        data = adb("shell", "cat", remote)
        # This smoke only accepts the no-account welcome, never a real workspace.
        document = ET.fromstring(data)
        node = next((n for n in document.iter("node") if n.get("text") == "Sign in with Internet Identity"), None)
        if node is not None:
            (args.output / name).write_text(data)
        return node

    results = []
    for index in range(3):
        adb("shell", "am", "force-stop", package)
        started = time.monotonic()
        launched = adb("shell", "am", "start", "-W", "-n", component)
        (args.output / f"launch-{index + 1}.txt").write_text(launched)
        if "Status: ok" not in launched or "LaunchState: COLD" not in launched:
            raise RuntimeError("The ordinary app did not cold-start")
        node = None
        while time.monotonic() - started < 25:
            node = tree(f"welcome-{index + 1}.xml")
            if node is not None:
                break
            time.sleep(0.2)
        if node is None:
            raise RuntimeError("The no-account welcome did not become available; no account actions were performed")
        results.append({"am_total_ms": int(re.search(r"TotalTime: (\d+)", launched)[1]),
                        "welcome_observed_ms": round((time.monotonic() - started) * 1000)})

    report = {"package": package, "api": int(adb("shell", "getprop", "ro.build.version.sdk").strip()),
              "page_size": int(adb("shell", "getconf", "PAGE_SIZE").strip()), "cold_starts": results,
              "timing_scope": "am force-stop cold process, warm OS caches; welcome observation includes UI dump overhead",
              "login_probe": "not requested", "credentials_entered": False}
    if args.login_probe:
        x1, y1, x2, y2 = map(int, re.findall(r"\d+", node.get("bounds")))
        adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))
        for attempt in range(40):
            # Never persist the login fragment, browser state or complete Intent.
            activity = adb("shell", "dumpsys", "activity", "activities")
            resumed = [line for line in activity.splitlines() if "mResumedActivity:" in line or "topResumedActivity=" in line]
            if "https://dmsg.net/_/login" in activity and any("com.android.chrome/" in line for line in resumed):
                report["login_probe"] = "native public-deployment preflight opened Chrome"
                break
            time.sleep(0.5)
        else:
            raise RuntimeError("Public deployment preflight did not open the expected browser; no credentials were entered")
        adb("shell", "input", "keyevent", "4")
    adb("shell", "am", "force-stop", package)
    (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
