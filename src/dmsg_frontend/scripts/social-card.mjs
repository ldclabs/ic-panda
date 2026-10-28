// Export the live hero's vector artwork with the registered localized catalogs.
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
const { locales, localeDir } = await readModule('../src/lib/i18n/locales.ts')
const requested = process.argv.slice(2)
if (requested.some((locale) => !locales.includes(locale))) throw new Error('Unsupported locale')
// Measure with the same Pango font engine used by sharp's SVG renderer.
// Character counts underestimate long words and contextual glyph shaping.
const widths = new Map()
async function measure(text, size) {
  const key = `${size}:${text}`
  if (!widths.has(key)) {
    const { width } = await sharp({ text: { text: escape(text), font: `Helvetica Neue Bold ${size}`, dpi: 72 } }).metadata()
    widths.set(key, width ?? 0)
  }
  return widths.get(key)
}
async function wrap(text, size, width, locale) {
  const words = []
  for (const part of new Intl.Segmenter(locale, { granularity: 'word' }).segment(text)) {
    if (!part.isWordLike && !/\s/.test(part.segment) && words.length) words[words.length - 1] += part.segment
    else words.push(part.segment)
  }
  const lines = ['']
  for (const word of words) {
    const last = lines.length - 1
    const next = `${lines[last]}${word}`
    if (lines[last] && await measure(next, size) > width) lines.push(word.trimStart())
    else lines[last] = next
  }
  return lines.map((line) => line.trim()).filter(Boolean)
}
async function fits(lines, size, width) {
  return lines.length <= 2 && (await Promise.all(lines.map((line) => measure(line, size)))).every((measured) => measured <= width)
}
for (const locale of requested.length ? requested : locales) {
  const { default: copy } = await readModule(
    `../src/lib/i18n/messages/${locale}.ts`
  )
  const rtl = localeDir(locale) === 'rtl'
  const direction = rtl ? 'rtl' : 'ltr'
  const x = rtl ? 1136 : 64
  let titleSize = locale.startsWith('zh') ? 82 : locale === 'ru' ? 54 : locale === 'ar' ? 72 : 78
  let first = await wrap(copy['Private collaboration.'], titleSize, 510, locale)
  while (titleSize > 32 && !(await fits(first, titleSize, 510))) {
    titleSize -= 2
    first = await wrap(copy['Private collaboration.'], titleSize, 510, locale)
  }
  let secondSize = locale.startsWith('zh') ? 54 : locale === 'ar' ? 49 : 42
  let second = await wrap(copy['For people and agents.'], secondSize, 540, locale)
  while (secondSize > 24 && !(await fits(second, secondSize, 540))) {
    secondSize -= 2
    second = await wrap(copy['For people and agents.'], secondSize, 540, locale)
  }
  const firstY = first.length === 1 ? 238 : 206
  const secondY = firstY + (first.length - 1) * (titleSize + 7) + 77
  const text = (content, y, size, color, weight = 600) =>
    `<text x="${x}" y="${y}" direction="${direction}" text-anchor="start" font-size="${size}" font-weight="${weight}" fill="${color}">${escape(content)}</text>`
  const art = renderHeroArt({
    prefix: `social-${locale}`,
    labels: {
      people: copy['People and agents'],
      space: copy['Private space'],
      network: copy['Agent network'],
      you: copy.You,
      desktop: copy['Desktop and terminal'],
      teammate: copy.Teammate,
      agent: copy['Your agent'],
      website: copy.Website
    },
    icons: glyphs,
    rtl,
    waiting: true
  }).replace('<svg ', '<svg x="0" y="0" ')
  // The request waiting at the gem, as the approval window shows it.
  const ticketX = rtl ? 96 : 724
  const ticketText = (content, dx, y, size, color, weight = 500) =>
    `<text x="${ticketX + (rtl ? 380 - dx : dx)}" y="${y}" direction="${direction}" text-anchor="start" font-size="${size}" font-weight="${weight}" fill="${color}">${escape(content)}</text>`
  const ticket = `<g font-family="Inter, -apple-system, Helvetica Neue, Arial, PingFang SC, sans-serif">
  <rect x="${ticketX}" y="438" width="380" height="128" rx="12" fill="#fff" stroke="#c8d5ca"/>
  <path d="M${ticketX} 450a12 12 0 0 1 12-12h356a12 12 0 0 1 12 12v18H${ticketX}Z" fill="#f0f4ee"/>
  <path d="M${ticketX} 468h380" stroke="#dae3d9"/>
  <g fill="#cfd9cf"><circle cx="${ticketX + 16}" cy="453" r="4"/><circle cx="${ticketX + 29}" cy="453" r="4"/><circle cx="${ticketX + 42}" cy="453" r="4"/></g>
  <text x="${ticketX + 190}" y="457" text-anchor="middle" font-size="12" font-weight="600" fill="#4e6257">${escape(copy['dMsg · Approval'])}</text>
  <path d="M${ticketX + (rtl ? 362 : 18)} 485l4.5 4.5-4.5 4.5-4.5-4.5Z" fill="#b45309"/>
  ${ticketText(copy.Signature, 30, 494, 13, '#92400e', 600)}
  ${ticketText(copy['Sign the release statement'], 16, 519, 17, '#10251f', 650)}
  <rect x="${rtl ? ticketX + 16 : ticketX + 176}" y="530" width="88" height="26" rx="7" fill="#fff" stroke="#c8d5ca"/>
  <rect x="${rtl ? ticketX + 112 : ticketX + 272}" y="530" width="92" height="26" rx="7" fill="#145c45"/>
  <text x="${rtl ? ticketX + 60 : ticketX + 220}" y="547" text-anchor="middle" font-size="12" font-weight="550" fill="#10251f">${escape(copy.Reject)}</text>
  <text x="${rtl ? ticketX + 158 : ticketX + 318}" y="547" text-anchor="middle" font-size="12" font-weight="550" fill="#fff">${escape(copy.Approve)}</text>
  </g>`
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
  <g transform="translate(${rtl ? 2 : 584} 66) scale(.96)">${art}</g>
  ${ticket}
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
