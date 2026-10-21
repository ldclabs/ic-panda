/**
 * The same vector scene appears in the live hero and the exported social cards.
 * All labels come from the local catalogs; escape them before building SVG.
 * @param {{ prefix: string, labels: { space: string, network: string, teammate: string, agent: string, you: string, owner: string, scope: string }, icons: Record<string, string>, rtl?: boolean }} options
 */
export function renderHeroArt({ prefix, labels, icons, rtl = false }) {
  const id = prefix.replace(/[^a-zA-Z0-9-]/g, '')
  const esc = (/** @type {string} */ s) =>
    s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;')
  const icon = (
    /** @type {string} */ name,
    /** @type {number} */ x,
    /** @type {number} */ y,
    size = 20,
    fill = '#22684e'
  ) =>
    `<g transform="translate(${x} ${y}) scale(${size / 24})" fill="${fill}"><path d="${icons[name]}"/></g>`
  const labelSize = (/** @type {string} */ text, width = 115, size = 16) =>
    Math.min(
      size,
      width /
        [...text].reduce(
          (sum, c) =>
            sum +
            (/[\u2e80-\u9fff]/.test(c)
              ? 1
              : /[MW@ЖШЩЮФ]/.test(c)
                ? 0.85
                : /[il., ]/.test(c)
                  ? 0.28
                  : 0.55),
          0
        )
    )
  const label = (
    /** @type {string} */ text,
    /** @type {number} */ x,
    /** @type {number} */ y,
    width = 115,
    fill = '#193c2d'
  ) =>
    `<text x="${rtl ? x + width : x}" y="${y}" font-size="${labelSize(text, width)}" font-weight="600" fill="${fill}" direction="${rtl ? 'rtl' : 'ltr'}" text-anchor="start">${esc(text)}</text>`
  // People collaborate in the private space. The agent has a separate route
  // to protocol services; the space itself never fans out into those services.
  const paths = [
    'M 194 108 C 230 108 228 170 269 207',
    'M 196 224 C 230 224 242 252 284 262',
    'M 207 375 C 249 375 224 296 284 265',
    'M 207 389 C 268 465 421 466 450 383 L 450 202',
    'M 450 202 H 469',
    'M 450 277 H 469',
    'M 450 352 H 469'
  ]
  const routes = paths
    .map(
      (d, i) =>
        `<g><path d="${d}" fill="none" stroke="${i === 2 ? '#c5ab7c' : '#aec7b5'}" stroke-width="1.3"/><path class="art-packet" d="${d}" pathLength="100" fill="none" stroke="${i === 2 ? '#b87124' : '#398465'}" stroke-width="3.4" stroke-linecap="round" stroke-dasharray="3 97" stroke-dashoffset="${-15 - i * 10}" style="--duration:${i < 3 ? 6.8 + i * 0.6 : i === 3 ? 10 : 3}s;--delay:${-i * 1.4}s"/></g>`
    )
    .join('')
  const post =
    'M4 0h4.5A1.5 1.5 0 0 1 10 1.5v45A1.5 1.5 0 0 1 8.5 48H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4Z'
  const dome =
    'M15.5 0H18a24 24 0 0 1 0 48h-2.5a1.5 1.5 0 0 1-1.5-1.5v-45A1.5 1.5 0 0 1 15.5 0Z'
  const gem = 'M12 17.5 18.5 24 12 30.5 5.5 24Z'
  return `<svg xmlns="http://www.w3.org/2000/svg" class="living-art" direction="ltr" viewBox="0 0 640 520" width="640" height="520" aria-hidden="true" focusable="false">
  <defs>
    <radialGradient id="${id}-wash"><stop stop-color="#dbe9d9"/><stop offset="1" stop-color="#f7f8f4" stop-opacity="0"/></radialGradient>
    <linearGradient id="${id}-forest" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#287c5b"/><stop offset=".55" stop-color="#175d43"/><stop offset="1" stop-color="#103c2e"/></linearGradient>
    <linearGradient id="${id}-gold" x1="0" y1="0" x2="1" y2="1"><stop stop-color="#e8bb6f"/><stop offset=".5" stop-color="#c58a38"/><stop offset="1" stop-color="#a55d1d"/></linearGradient>
    <pattern id="${id}-dots" width="22" height="22" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r=".65" fill="#a6bdac"/></pattern>
    <filter id="${id}-paper" x="-30%" y="-50%" width="160%" height="220%"><feDropShadow dx="0" dy="9" stdDeviation="10" flood-color="#214e36" flood-opacity=".10"/></filter>
    <filter id="${id}-depth" x="-70%" y="-50%" width="240%" height="220%"><feDropShadow dx="1" dy="4" stdDeviation="3.3" flood-color="#153c28" flood-opacity=".22"/></filter>
    <mask id="${id}-cut" maskUnits="userSpaceOnUse" x="0" y="0" width="42" height="48"><rect width="42" height="48" fill="white"/><path d="${gem}" stroke="black" stroke-width="4" stroke-linejoin="round"/></mask>
  </defs>
  <ellipse cx="323" cy="267" rx="281" ry="237" fill="url(#${id}-wash)"/>
  <ellipse cx="324" cy="259" rx="235" ry="211" fill="url(#${id}-dots)" opacity=".42"/>
  <g fill="none" stroke="#b5c8af" stroke-width=".8" opacity=".5"><path d="M323 66V88M216 412h212M220 126h-12v18M426 126h12v18M220 401h-12v-18M426 401h12v-18"/></g>
  <g fill="#9bb697"><circle cx="390" cy="62" r="2"/><circle cx="218" cy="53" r="2"/><circle cx="602" cy="424" r="2.5"/><path d="M575 88v10m-5-5h10M116 457v10m-5-5h10" stroke="#a8ba95" fill="none"/></g>
  <rect x="446" y="152" width="189" height="241" rx="22" fill="#eaf0e4" fill-opacity=".52" stroke="#c2d2bb" stroke-dasharray="3 7"/>
  ${routes}
  <ellipse cx="323" cy="402" rx="101" ry="10" fill="#244e35" opacity=".06"/>
  <g class="art-core" transform="translate(230 155)">
    <g transform="scale(4.45)" filter="url(#${id}-depth)"><g mask="url(#${id}-cut)" fill="url(#${id}-forest)"><path d="${post}"/><path d="${dome}"/></g><path d="${gem}" fill="#f7f8f4" stroke="#e1c998" stroke-width=".5"/><path class="art-gem" d="${gem}" transform="translate(1.2 2.4) scale(.9)" fill="url(#${id}-gold)"/><path d="M2 6V40M17 3h1a21 21 0 0 1 18 11" fill="none" stroke="#6cac80" stroke-width=".35" opacity=".55"/></g>
    <g opacity=".72" fill="#91bc9b"><rect x="88" y="38" width="63" height="29" rx="7" opacity=".2"/><rect x="94" y="78" width="69" height="29" rx="7" opacity=".2"/><rect x="88" y="118" width="63" height="29" rx="7" opacity=".2"/>${icon('lock', 97, 44, 16, '#c1dcc4')}${icon('file', 103, 84, 16, '#c1dcc4')}${icon('chat', 97, 124, 16, '#c1dcc4')}<path d="M120 49h22m-22 6h15m4 34h14m-14 6h21m-21 34h20m-20 6h12" stroke="#b4d4b9" stroke-width="1.6" stroke-linecap="round"/></g>
  </g>
  <g font-family="Inter, Helvetica Neue, Arial, PingFang SC, sans-serif">
    <text x="324" y="119" text-anchor="middle" direction="${rtl ? 'rtl' : 'ltr'}" font-size="${labelSize(labels.space, 195, 16)}" font-weight="600" fill="#32583b">${esc(labels.space)}</text>
    <text x="541" y="135" text-anchor="middle" direction="${rtl ? 'rtl' : 'ltr'}" font-size="${labelSize(labels.network, 178, 16)}" font-weight="600" fill="#32583b">${esc(labels.network)}</text>
    <g transform="translate(10 68) rotate(-5 94 38)"><g class="art-float art-float-one" filter="url(#${id}-paper)"><rect x="5" y="6" width="185" height="76" rx="13" fill="#dfebdc"/><rect width="185" height="76" rx="13" fill="#fff" stroke="#c9d8c8"/><rect x="14" y="18" width="39" height="39" rx="11" fill="#eaf2e7"/>${icon('fingerprint', 22, 26, 23)}${label(labels.you, 64, 33, 108)}<text x="${rtl ? 172 : 64}" y="55" direction="${rtl ? 'rtl' : 'ltr'}" text-anchor="start" font-size="12" fill="#68816c">${esc(labels.owner)}</text><circle cx="173" cy="12" r="4" fill="#e5bd74"/></g></g>
    <g transform="translate(12 185) rotate(3 94 38)"><g class="art-float art-float-two" filter="url(#${id}-paper)"><rect width="185" height="76" rx="13" fill="#fff" stroke="#c9d8c8"/><rect x="14" y="18" width="39" height="39" rx="11" fill="#eaf2e7"/>${icon('members', 21, 25, 23)}${label(labels.teammate, 64, 33, 108)}<text x="${rtl ? 172 : 64}" y="55" text-anchor="${rtl ? 'end' : 'start'}" font-size="12" fill="#68816c">Mei</text><circle cx="169" cy="63" r="3" fill="#559f72"/></g></g>
    <g transform="translate(12 338) rotate(-4 99 40)"><g class="art-float art-float-three" filter="url(#${id}-paper)"><rect x="5" y="6" width="196" height="82" rx="13" fill="#e6dac0"/><rect width="196" height="82" rx="13" fill="#193e30" stroke="#2c6248"/><rect x="14" y="20" width="39" height="40" rx="11" fill="#315c43"/>${icon('robot', 22, 28, 23, '#d2e7ce')}${label(labels.agent, 64, 35, 120, '#f5f6e9')}<text x="${rtl ? 184 : 64}" y="58" text-anchor="${rtl ? 'end' : 'start'}" font-size="12" fill="#bbd4bc" letter-spacing=".6">Agent ID</text></g></g>
    ${[
      { name: 'Agent Discourse', glyph: 'discuss', y: 172 },
      { name: 'Agent Mail', glyph: 'mail', y: 247 },
      { name: 'Agent Knowledge', glyph: 'book', y: 322 }
    ]
      .map(
        (service, i) =>
          `<g transform="translate(465 ${service.y}) rotate(${i === 1 ? 2 : -2} 82 26)"><g class="art-float art-float-${['one', 'two', 'three'][i]}" filter="url(#${id}-paper)"><rect width="164" height="54" rx="11" fill="#fff" stroke="#c9d8c8"/>${icon(service.glyph, 12, 17, 20)}<text x="41" y="33" font-size="${i === 1 ? 14 : 12.5}" font-weight="500" fill="#244b37">${service.name}</text></g></g>`
      )
      .join('')}
    <g transform="translate(257 385)"><rect width="143" height="25" rx="12" fill="#f8f8ef" stroke="#c7d3b8"/>${icon('shield', 10, 6, 13, '#77895a')}<text x="86" y="17" text-anchor="middle" font-size="${labelSize(labels.scope, 100, 11)}" fill="#5c7048" direction="${rtl ? 'rtl' : 'ltr'}">${esc(labels.scope)}</text></g>

  </g>
  </svg>`
}
