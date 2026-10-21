// Regenerate the six social cards from the same copy used by the landing page.
// sharp is provided by the existing PWA asset generator dependency.
import { readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
const require = createRequire(import.meta.url)
const assets = createRequire(require.resolve('@vite-pwa/assets-generator'))
const sharp = assets('sharp')
const ts = require('typescript')
const directory = new URL('../static/_assets/dmsg/', import.meta.url)
const mark = (await readFile(new URL('mark.svg', directory))).toString('base64')
const escape = (s) =>
  s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;')
for (const locale of ['en', 'zh', 'fr', 'es', 'ru', 'ar']) {
  const source = await readFile(
    new URL(`../src/lib/i18n/messages/${locale}.ts`, import.meta.url),
    'utf8'
  )
  const { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext }
  })
  const { default: copy } = await import(
    `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
  )
  const headline = [
    copy['Private collaboration.'],
    copy['For people and agents.']
  ]
  const rtl = locale === 'ar'
  const x = rtl ? 1120 : 80
  const direction = rtl ? 'rtl' : 'ltr'
  const anchor = 'start'
  const size = ['fr', 'ru'].includes(locale) ? 60 : locale === 'ar' ? 66 : 72
  const labels = ['Private space', 'Collaboration', 'Agent network'].map(
    (k) => copy[k]
  )
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#f7f8f4"/>
  <image href="data:image/svg+xml;base64,${mark}" x="80" y="54" width="35" height="40"/>
  <g font-family="Helvetica Neue, Arial, PingFang SC, sans-serif" fill="#10251f">
  <text x="132" y="86" font-size="34" font-weight="700">dMsg</text>
  <text x="1120" y="82" text-anchor="end" font-size="19" fill="#4e6257">dmsg.net</text>
  <text x="${x}" y="245" text-anchor="${anchor}" direction="${direction}" unicode-bidi="plaintext" font-size="${size}" font-weight="700">${escape(headline[0])}</text>
  <text x="${x}" y="340" text-anchor="${anchor}" direction="${direction}" unicode-bidi="plaintext" font-size="${size}" font-weight="700" fill="#145c45">${escape(headline[1])}</text>
  <path d="M80 424H1120" stroke="#c8d5ca"/>
  ${labels.map((label, i) => `<text x="${rtl ? 1120 - i * 355 : 80 + i * 355}" y="490" text-anchor="${anchor}" direction="${direction}" unicode-bidi="plaintext" font-size="${locale === 'ru' ? 24 : 26}" font-weight="500">${escape(label)}</text>`).join('\n')}
  <text x="80" y="566" font-size="19" fill="#4e6257">macOS · Windows · Linux CLI</text>
  </g></svg>`
  const stem = locale === 'en' ? 'og-image' : `og-image-${locale}`
  await writeFile(new URL(`${stem}.svg`, directory), svg)
  await sharp(Buffer.from(svg))
    .png()
    .toFile(fileURLToPath(new URL(`${stem}.png`, directory)))
  console.log(`${stem}.png`)
}
