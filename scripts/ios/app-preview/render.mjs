// Renders the caption/end-card plates and the temp score from plate.html via headless Chrome.
//   node render.mjs OUT_DIR [zh en] [--frames=a,b,...] [--no-score]
// Writes OUT_DIR/plate-<lang>/%04d.png (885 frames, 30 fps) and OUT_DIR/score.wav.
import { spawn } from 'node:child_process';
import { mkdirSync, writeFileSync, mkdtempSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const out = resolve(args.find(a => !a.startsWith('--')) ?? 'build');
const langs = args.filter(a => a === 'zh' || a === 'en');
const only = args.find(a => a.startsWith('--frames='))?.slice(9).split(',').map(Number);
const chrome = process.env.DMSG_TEST_CHROME ?? `${process.env.HOME}/Library/Caches/ms-playwright/chromium-1243/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing`;
const FPS = 30, FRAMES = Math.round(29.5 * FPS);
// Output times of the taps in the cut (edit.py lands each tap here).
const TAPS = [0.75, 6.75, 12.75, 15.0, 18.75, 21.75, 23.0];

const profile = mkdtempSync(join(tmpdir(), 'plate-chrome-'));
const proc = spawn(chrome, ['--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--hide-scrollbars',
  '--force-color-profile=srgb', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
const wsUrl = await new Promise((ok, fail) => {
  let buf = '';
  proc.stderr.on('data', d => { buf += d; const m = buf.match(/DevTools listening on (ws:\S+)/); if (m) ok(m[1]); });
  proc.on('exit', c => fail(new Error(`chrome exited ${c}`)));
});
const browser = new WebSocket(wsUrl);
await new Promise(r => browser.addEventListener('open', r, { once: true }));
let id = 0; const pending = new Map();
browser.addEventListener('message', e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.fail(new Error(JSON.stringify(m.error))) : p.ok(m.result); } });
const send = (method, params = {}, sessionId) => new Promise((ok, fail) => { const i = ++id; pending.set(i, { ok, fail }); browser.send(JSON.stringify({ id: i, method, params, sessionId })); });

const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
const page = (m, p) => send(m, p, sessionId);
await page('Emulation.setDeviceMetricsOverride', { width: 886, height: 1920, deviceScaleFactor: 1, mobile: false });
await page('Page.enable');
await page('Page.navigate', { url: pathToFileURL(join(here, 'plate.html')).href });
const evaluate = async (expression) => {
  const r = await page('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
for (let i = 0; i < 100 && !(await evaluate('typeof renderAt === "function" && document.fonts.status === "loaded"')); i++) await new Promise(r => setTimeout(r, 50));

try {
  for (const lang of langs.length ? langs : ['zh', 'en']) {
    const dir = join(out, `plate-${lang}`); mkdirSync(dir, { recursive: true });
    for (const f of only ?? [...Array(FRAMES).keys()]) {
      await evaluate(`renderAt(${f / FPS}, ${JSON.stringify(lang)})`);
      const { data } = await page('Page.captureScreenshot', { format: 'png', clip: { x: 0, y: 0, width: 886, height: 1920, scale: 1 } });
      writeFileSync(join(dir, `${String(f).padStart(4, '0')}.png`), Buffer.from(data, 'base64'));
    }
    console.log(`plate-${lang}: ${(only ?? []).length || FRAMES} frames`);
  }
  if (!args.includes('--no-score')) {
    writeFileSync(join(out, 'score.wav'), Buffer.from(await evaluate(`renderScore(${JSON.stringify(TAPS)})`), 'base64'));
    console.log('score.wav');
  }
} finally {
  browser.close(); proc.kill();
}
