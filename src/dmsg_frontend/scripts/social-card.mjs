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
