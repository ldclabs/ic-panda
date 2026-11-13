# App Store preview edit

Cuts the takes recorded by `scripts/ios/record-app-preview.sh` into the 29.5 s
iPhone App Preview (886×1920, 30 fps, H.264 High@4.0 at 11 Mbps, AAC 48 kHz).

```bash
bash scripts/ios/record-app-preview.sh SIMULATOR_UUID misc/dmsg-ios-app-preview/raw
node scripts/ios/app-preview/render.mjs misc/dmsg-ios-app-preview/build
python3 scripts/ios/app-preview/edit.py misc/dmsg-ios-app-preview/raw misc/dmsg-ios-app-preview/build
```

- `plate.html` draws everything except the app screen: captions, the Plus
  disclosure, the end card, and a temporary score. Copy changes go in its
  `SCENES` and `TEXT`.
- `render.mjs` renders the plates (885 PNG frames per language) and
  `score.wav` with headless Chrome (`DMSG_TEST_CHROME` overrides the path).
- `edit.py` anchors each segment on the first frame that changes after a
  `PREVIEW-MARK` (the touch marker), dissolves between segments, rounds the
  screen corners and encodes `dmsg-preview-<lang>-iphone.mp4`.

The recording is variable frame rate and drops its still tail, so takes are
read with `fps=30,select=...` and padded with `tpad`, never seeked with `-ss`.
misc/ is ignored by Git, so raw takes and outputs stay local.
