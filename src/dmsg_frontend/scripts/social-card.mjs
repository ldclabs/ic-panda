// Export the live hero's vector artwork with the same six localized catalogs.
// sharp is provided by the existing PWA asset generator dependency.
import { readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { renderHeroArt } from '../src/lib/components/landing/hero-art.js'
const require = createRequire(import.meta.url)
const assets = createRequire(require.resolve('@vite-pwa/assets-generator'))
const sharp = assets('sharp')
const ts = require('typescript')
const directory = new URL('../static/_assets/dmsg/', import.meta.url)
const mark = (await readFile(new URL('mark.svg', directory))).toString('base64')
const escape = (s) =>
  s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;')
async function readModule(path) {
  const source = await readFile(new URL(path, import.meta.url), 'utf8')
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext }
  })
  return import(
    `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
  )
}
const { glyphs } = await readModule('../src/lib/components/landing/glyphs.ts')
const measure = (text, size) =>
  [...text].reduce(
    (sum, c) =>
      sum +
      (/\s/.test(c)
        ? 0.25
        : /[\u2e80-\u9fff]/.test(c)
          ? 1
          : /[MWЖШЩЮФ]/.test(c)
            ? 0.86
            : /[il.,!]/.test(c)
              ? 0.25
              : 0.56),
    0
  ) * size
function wrap(text, size, width) {
  const words = text.split(' ')
  const lines = ['']
  for (const word of words) {
    const last = lines.length - 1
    const next = `${lines[last]}${lines[last] ? ' ' : ''}${word}`
    if (lines[last] && measure(next, size) > width) lines.push(word)
    else lines[last] = next
  }
  return lines
}
for (const locale of ['en', 'zh', 'fr', 'es', 'ru', 'ar']) {
  const { default: copy } = await readModule(
    `../src/lib/i18n/messages/${locale}.ts`
  )
  const rtl = locale === 'ar'
  const direction = rtl ? 'rtl' : 'ltr'
  const x = rtl ? 1136 : 64
  const titleSize =
    locale === 'zh' ? 82 : locale === 'ru' ? 54 : locale === 'ar' ? 72 : 78
  const first = wrap(copy['Private collaboration.'], titleSize, 510)
  const secondSize = locale === 'zh' ? 54 : locale === 'ar' ? 49 : 42
  const second = wrap(copy['For people and agents.'], secondSize, 540)
  const firstY = first.length === 1 ? 238 : 206
  const secondY = firstY + (first.length - 1) * (titleSize + 7) + 77
  const text = (content, y, size, color, weight = 600) =>
    `<text x="${x}" y="${y}" direction="${direction}" text-anchor="start" font-size="${size}" font-weight="${weight}" fill="${color}">${escape(content)}</text>`
  const art = renderHeroArt({
    prefix: `social-${locale}`,
    labels: {
      space: copy['Private space'],
      network: copy['Agent network'],
      teammate: copy.Teammate,
      agent: copy['Your agent'],
      owner: copy.Owner,
      you: copy.You,
      scope: copy['Scoped access']
    },
    icons: glyphs,
    rtl
  }).replace('<svg ', '<svg x="0" y="0" ')
  const title = [
    ...first.map((line, i) =>
      text(line, firstY + i * (titleSize + 7), titleSize, '#123a2b', 700)
    ),
    ...second.map((line, i) =>
      text(line, secondY + i * (secondSize + 8), secondSize, '#28654a', 650)
    )
  ].join('')
  const labels = ['Private space', 'Collaboration', 'Agent network'].map(
    (k) => copy[k]
  )
  const bottomY = Math.max(
    447,
    secondY + (second.length - 1) * (secondSize + 8) + 68
  )
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <defs><linearGradient id="paper" x1="0" y1="0" x2="1" y2=".5"><stop stop-color="#f8f9f3"/><stop offset="1" stop-color="#edf3e8"/></linearGradient><pattern id="paper-dots" width="24" height="24" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r=".65" fill="#b6c8b2"/></pattern></defs>
  <rect width="1200" height="630" fill="url(#paper)"/>
  <rect x="${rtl ? 0 : 620}" width="580" height="630" fill="url(#paper-dots)" opacity=".42"/>
  <path d="M${rtl ? 580 : 620} 34V596" stroke="#d4dfcd" stroke-dasharray="2 7"/>
  <g transform="translate(${rtl ? -14 : 572} 63) scale(.98)">${art}</g>
  <image href="data:image/svg+xml;base64,${mark}" x="${rtl ? 1102 : 64}" y="48" width="31" height="36"/>
  <g font-family="Helvetica Neue, Arial, PingFang SC, sans-serif">
  <text x="${rtl ? 1086 : 110}" y="78" text-anchor="${rtl ? 'end' : 'start'}" font-size="31" font-weight="700" fill="#123a2b">dMsg</text>
  ${title}
  <g fill="#55715a">${labels.map((label, i) => text(label, bottomY + i * 28, 18, '#55715a', 450)).join('')}</g>
  <text x="${x}" y="581" text-anchor="${rtl ? 'end' : 'start'}" font-size="19" fill="#345940">dmsg.net</text>
  <text x="${rtl ? 64 : 1136}" y="584" text-anchor="end" direction="${direction}" font-size="13" fill="#62816b">${escape(copy['For people and agents.'])}</text>
  </g></svg>`
  const stem = locale === 'en' ? 'og-image' : `og-image-${locale}`
  await writeFile(new URL(`${stem}.svg`, directory), svg)
  await sharp(Buffer.from(svg))
    .png()
    .toFile(fileURLToPath(new URL(`${stem}.png`, directory)))
  console.log(`${stem}.png`)
}
