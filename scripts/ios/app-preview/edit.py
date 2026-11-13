#!/usr/bin/env python3
"""Cuts the raw simulator takes into the 29.5 s P1 preview and lays them over the plates.

    python3 edit.py RAW_DIR BUILD_DIR [zh en]

RAW_DIR holds the takes from scripts/ios/record-app-preview.sh. BUILD_DIR holds
plate-<lang>/%04d.png and score.wav from render.mjs; the video is written to
BUILD_DIR/dmsg-preview-<lang>-iphone.mp4. Each segment is anchored on the first
frame that changes after a PREVIEW-MARK, so taps land on the cut's beat.
"""
import os, subprocess, sys

W, H, FPS = 640, 1391, 30
TINY = (33, 72)

# (take, mark, output time of the tap, output start, output end, dissolve into next)
EDL = [
    ("Scene1Keep", "s1.unlock", 0.75, 0.0, 6.0, 0.333),
    ("Scene2Devices", "s2.item", 6.75, 6.0, 9.0, 0.2),
    ("Scene2Devices", "s2.devices", 9.0, 9.0, 12.0, 0.333),
    ("Scene3Send", "s3.file", 12.75, 12.0, 13.4, 0.2),
    ("Scene3Send", "s3.create", 15.0, 13.4, 18.0, 0.333),
    ("Scene4aRequest", "s4.create", 18.75, 18.0, 21.0, 0.2),
    ("Scene4bArrived", "s4.open", 21.75, 21.0, 22.4, 0.2),
    ("Scene4bArrived", "s4.save", 23.0, 22.4, 24.5, 0),
]
# The devices screen is already open at its mark; start a beat after it.
HOLD_AFTER = {"s2.devices": 0.3}


def marks(path):
    out = {}
    for line in open(path):
        t, name = line.split()
        out[name] = float(t)
    return out


def first_change(mov, t):
    """Time of the first frame after t that differs from the frame at t (the touch marker counts)."""
    w, h = TINY
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", mov, "-vf",
        f"fps={FPS},select='gte(t,{t - 0.05})',scale={w}:{h},format=gray", "-frames:v", "75",
        "-f", "rawvideo", "-"], check=True, capture_output=True).stdout
    frames = [raw[i:i + w * h] for i in range(0, len(raw) - w * h + 1, w * h)]
    for i in range(1, len(frames)):
        if sum(abs(a - b) for a, b in zip(frames[0], frames[i])) > 50:
            return t - 0.05 + i / FPS
    raise SystemExit(f"no change after {t:.3f} in {mov}")


def mask(build):
    path = os.path.join(build, "mask.png")
    if not os.path.exists(path):
        r = 56
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", f"color=black:s={W}x{H}", "-frames:v", "1", "-vf",
            f"format=gray,geq=lum='255*clip(({r}+0.5-hypot(max(0,max({r}-X,X-{W - 1 - r})),max(0,max({r}-Y,Y-{H - 1 - r})))),0,1)'",
            path], check=True)
    return path


def build(raw, build_dir, lang):
    inputs, chains, cache = [], [], {}
    for n, (take, mark, anchor, a, b, _) in enumerate(EDL):
        mov = os.path.join(raw, f"{lang}-{take}.mov")
        key = (mov, mark)
        if key not in cache:
            m = marks(os.path.join(raw, f"{lang}-{take}.marks"))[mark]
            cache[key] = m + HOLD_AFTER[mark] if mark in HOLD_AFTER else first_change(mov, m)
        prev = EDL[n - 1][5] if n else 0
        start_out = a - prev / 2
        end_out = b + EDL[n][5] / 2
        src = cache[key] + (start_out - anchor)
        print(f"{lang} {take:15} {mark:11} tap@{cache[key]:7.3f}s  src {src:7.3f}+{end_out - start_out:5.3f} -> {start_out:6.3f}")
        inputs += ["-i", mov]
        chains.append(f"[{n}:v]fps={FPS},select='gte(t,{src:.4f})',setpts=PTS-STARTPTS,tpad=stop_mode=clone:stop_duration=4,"
                      f"trim=duration={end_out - start_out:.4f},setpts=PTS-STARTPTS,scale={W}:{H}:flags=lanczos,format=yuv420p,setsar=1[s{n}]")
    cur, t = "s0", None
    for n in range(1, len(EDL)):
        d = EDL[n - 1][5]
        offset = EDL[n][3] - d / 2
        chains.append(f"[{cur}][s{n}]xfade=transition=fade:duration={d}:offset={offset:.4f}[x{n}]")
        cur = f"x{n}"
    k = len(EDL)
    inputs += ["-loop", "1", "-i", mask(build_dir), "-framerate", str(FPS), "-i", os.path.join(build_dir, f"plate-{lang}", "%04d.png"),
               "-i", os.path.join(build_dir, "score.wav")]
    chains.append(f"[{cur}]format=rgba[scr];[{k}:v]format=gray,trim=duration=24.5[m];[scr][m]alphamerge,"
                  f"fade=t=out:st=24.0:d=0.5:alpha=1,tpad=stop_mode=add:stop_duration=6:color=black@0[layer]")
    chains.append(f"[{k + 1}:v]format=rgba[plate];[plate][layer]overlay=123:430:eof_action=pass:format=auto,format=yuv420p[v]")
    out = os.path.join(build_dir, f"dmsg-preview-{lang}-iphone.mp4")
    subprocess.run(["ffmpeg", "-v", "error", "-y", *inputs, "-filter_complex", ";".join(chains), "-map", "[v]", "-map", f"{k + 2}:a",
        "-t", "29.5", "-r", str(FPS), "-c:v", "libx264", "-profile:v", "high", "-level", "4.0", "-pix_fmt", "yuv420p",
        "-preset", "slow", "-b:v", "11M", "-minrate", "11M", "-maxrate", "11M", "-bufsize", "11M", "-x264-params", "nal-hrd=cbr",
        "-c:a", "aac", "-b:a", "256k", "-ar", "48000", "-ac", "2", "-movflags", "+faststart", out], check=True)
    print(out)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    for lang in sys.argv[3:] or ["zh", "en"]:
        build(sys.argv[1], sys.argv[2], lang)
