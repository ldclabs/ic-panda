// Add a legible padlock to the existing tray artwork without changing its mark
// or approval badge. Development-only prerequisite: librsvg's rsvg-convert.
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const resources = new URL('../resources/', import.meta.url)
// Filled lock body, open shackle and a transparent keyhole, in an 18px canvas.
const lock = 'M12 12V10.5a2.5 2.5 0 0 1 5 0V12h.25a.75.75 0 0 1 .75.75v4.5a.75.75 0 0 1-.75.75h-5.5a.75.75 0 0 1-.75-.75v-4.5a.75.75 0 0 1 .75-.75Zm1.25 0h2.5v-1.5a1.25 1.25 0 0 0-2.5 0Zm.5 2v2h1.5v-2Z'
for (const template of [false, true]) {
  for (const waiting of [false, true]) {
    for (const scale of [1, 2]) {
      const suffix = `${template ? 'Template' : ''}${scale === 2 ? '@2x' : ''}.png`
      const original = readFileSync(new URL(`tray${waiting ? 'Waiting' : ''}${suffix}`, resources)).toString('base64')
      const size = (template ? 18 : 16) * scale
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${size}" height="${size}" viewBox="0 0 18 18">
        <defs><mask id="cut"><rect width="18" height="18" fill="white"/><path d="${lock}" fill="black" stroke="black" stroke-width="1.8" stroke-linejoin="round"/></mask></defs>
        <image width="18" height="18" xlink:href="data:image/png;base64,${original}" mask="url(#cut)"/>
        <path d="${lock}" fill="${template ? '#000' : '#145c45'}" fill-rule="evenodd"/>
      </svg>`
      const result = spawnSync('rsvg-convert', [], { input: svg })
      if (result.error || result.status !== 0) throw result.error ?? new Error(result.stderr.toString())
      writeFileSync(fileURLToPath(new URL(`trayLocked${waiting ? 'Waiting' : ''}${suffix}`, resources)), result.stdout)
    }
  }
}
