/**
 * The gatehouse: the dMsg mark drawn at figure scale, with the people and
 * agents who ask on the left, the rooms of the private space in the dome and
 * the agent network on the right. Every request reaches the dome through the
 * gap, and the agent's signed messages leave through it. The live hero and
 * the six social cards render this same scene; labels come from the catalogs
 * and are escaped here.
 */

// The mark's own geometry (static/_assets/dmsg/mark.svg) on its 42 × 48 grid.
const POST =
  'M4 0h4.5A1.5 1.5 0 0 1 10 1.5v45A1.5 1.5 0 0 1 8.5 48H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4Z'
const DOME =
  'M15.5 0H18a24 24 0 0 1 0 48h-2.5a1.5 1.5 0 0 1-1.5-1.5v-45A1.5 1.5 0 0 1 15.5 0Z'
const GEM = 'M12 17.5 18.5 24 12 30.5 5.5 24Z'
export const MARK = { post: POST, dome: DOME, gem: GEM }

export const ASKERS = /** @type {const} */ ([
  'you',
  'teammate',
  'agent',
  'website'
])
export const ROOMS = /** @type {const} */ ([
  'vault',
  'files',
  'channels',
  'wallets',
  'agent'
])
export const HOSTS = /** @type {const} */ (['mail', 'discourse', 'knowledge'])

const ROOM_GLYPH = {
  vault: 'vault',
  files: 'folder',
  channels: 'chat',
  wallets: 'coins',
  agent: 'fingerprint'
}
const HOST_CARDS = [
  { glyph: 'mail', title: 'Agent Mail', host: 'mail.dmsg.net' },
  { glyph: 'discuss', title: 'Agent Discourse', host: 'discourse.dmsg.net' },
  { glyph: 'book', title: 'Agent Knowledge', host: 'knowledge.dmsg.net' }
]

/**
 * Where everything sits. Wide: askers, the gate and the network side by
 * side. Narrow: the network moves under the gate, so a phone keeps the text
 * readable.
 * @param {boolean} narrow
 */
function geometry(narrow) {
  const g = narrow
    ? {
        width: 360,
        height: 352,
        top: 0,
        s: 3.5,
        mx: 200,
        my: 56,
        asker: { x: 4, w: 150, h: 48, ys: [30, 86, 142, 198] },
        trace: { x: 162, dx: 7, y: 16, dy: 8 },
        host: { w: 112, h: 52, xs: [4, 124, 244], ys: [288, 288, 288] },
        busY: 268,
        room: { radius: 56, size: 26 },
        labels: {
          people: { x: 4, y: 18, w: 150 },
          space: { x: 298, y: 46, w: 110 },
          network: { x: 4, y: 258, w: 150 }
        }
      }
    : {
        width: 640,
        height: 364,
        top: 44,
        s: 4.6,
        mx: 228,
        my: 140,
        asker: { x: 8, w: 168, h: 54, ys: [100, 176, 252, 328] },
        trace: { x: 190, dx: 9, y: 72, dy: 11 },
        host: { w: 174, h: 52, xs: [458, 458, 458], ys: [166, 240, 314] },
        busX: 446,
        busY: 392,
        room: { radius: 72, size: 32 },
        labels: {
          people: { x: 8, y: 86, w: 168 },
          space: { x: 356.8, y: 128, w: 150 },
          network: { x: 458, y: 152, w: 174 }
        }
      }
  const at = (/** @type {number} */ u) => +(g.mx + u * g.s).toFixed(1)
  const gapX = at(12)
  const gemY = +(g.my + 24 * g.s).toFixed(1)
  const domeX = at(18)
  const { asker, host, trace } = g

  // Rooms sit on an arc around the dome's centre, clear of the gem's cut.
  const rooms = ROOMS.map((id, i) => {
    const angle = ((i - 2) * 32 * Math.PI) / 180
    return {
      id,
      x: +(domeX + g.room.radius * Math.cos(angle)).toFixed(1),
      y: +(gemY + g.room.radius * Math.sin(angle)).toFixed(1)
    }
  })

  // Each asker's trace runs right, rises over the post and drops down the
  // gap to the gem. Lower cards turn later and rise less, so no two cross.
  const askers = ASKERS.map((_, i) => {
    const cy = asker.ys[i] + asker.h / 2
    const x = trace.x + i * trace.dx
    const y = trace.y + i * trace.dy
    return `M${asker.x + asker.w} ${cy}H${x - 8}Q${x} ${cy} ${x} ${cy - 8}V${y + 8}Q${x} ${y} ${x + 8} ${y}H${gapX - 8}Q${gapX} ${y} ${gapX} ${y + 8}V${gemY}`
  })

  // From the gem down the gap, then under the dome to a host: up a bus on
  // the right when wide, down to a row of hosts when narrow.
  const hosts = HOSTS.map((_, i) => {
    const down = `M${gapX} ${gemY}V${g.busY - 10}`
    if (g.busX === undefined) {
      const cx = host.xs[i] + host.w / 2
      const side = cx < gapX ? -10 : 10
      return `${down}Q${gapX} ${g.busY} ${gapX + side} ${g.busY}H${cx - side}Q${cx} ${g.busY} ${cx} ${g.busY + 10}V${host.ys[i]}`
    }
    const cy = host.ys[i] + host.h / 2
    return `${down}Q${gapX} ${g.busY} ${gapX + 10} ${g.busY}H${g.busX - 10}Q${g.busX} ${g.busY} ${g.busX} ${g.busY - 10}V${cy + 8}Q${g.busX} ${cy} ${g.busX + 8} ${cy}H${host.xs[i]}`
  })

  // From the gem's right point across the dome to a room.
  const sx = at(18.5)
  const corridors = rooms.map((room) => {
    const ex = room.x - g.room.size / 2
    return `M${sx} ${gemY}C${sx + 14} ${gemY} ${ex - 12} ${room.y} ${ex} ${room.y}`
  })

  return { ...g, gapX, gemY, rooms, routes: { askers, hosts, corridors } }
}

/**
 * @param {{
 *   prefix: string,
 *   labels: { people: string, space: string, network: string, you: string, desktop: string, teammate: string, agent: string, website: string },
 *   icons: Record<string, string>,
 *   rtl?: boolean,
 *   narrow?: boolean,
 *   live?: boolean,
 *   waiting?: boolean
 * }} options
 */
export function renderHeroArt({
  prefix,
  labels,
  icons,
  rtl = false,
  narrow = false,
  live = false,
  waiting = false
}) {
  const id = prefix.replace(/[^a-zA-Z0-9-]/g, '')
  const g = geometry(narrow)
  const esc = (/** @type {string} */ s) =>
    s.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;')
  const icon = (
    /** @type {string} */ name,
    /** @type {number} */ x,
    /** @type {number} */ y,
    size = 20,
    cls = ''
  ) =>
    `<path${cls ? ` class="${cls}"` : ''} transform="translate(${x} ${y}) scale(${size / 24})" d="${icons[name]}"/>`
  // A rough advance width per character, enough to shrink a long label.
  const fit = (
    /** @type {string} */ text,
    width = 110,
    size = 14,
    mono = false
  ) =>
    +Math.min(
      size,
      width /
        [...text].reduce(
          (sum, c) =>
            sum +
            (mono
              ? 0.64
              : /[⺀-鿿]/.test(c)
                ? 1
                : /[MW@ЖШЩЮФ]/.test(c)
                  ? 0.86
                  : /[il.,:\- ]/.test(c)
                    ? 0.3
                    : 0.56),
          0
        )
    ).toFixed(2)
  // The diagram stays LTR. In Arabic, card text aligns to the card's right
  // edge; identifiers (latin) keep their own direction.
  const text = (
    /** @type {string} */ value,
    /** @type {number} */ x,
    /** @type {number} */ y,
    /** @type {number} */ width,
    /** @type {number} */ size,
    /** @type {string} */ cls,
    latin = false
  ) =>
    `<text class="${cls}" x="${rtl ? x + width : x}" y="${y}" font-size="${fit(value, width, size, cls === 'art-host-name')}" direction="${rtl && !latin ? 'rtl' : 'ltr'}" text-anchor="${rtl && latin ? 'end' : 'start'}">${esc(value)}</text>`
  const check = (/** @type {number} */ x) =>
    `<path class="art-check" d="M${x} 9.5l3 3 5.5-6"/>`

  // Names and hosts are identifiers and stay left to right.
  const { asker, host } = g
  const tileSize = asker.h - 20
  const textX = 10 + tileSize + 10
  const textW = asker.w - textX - 12
  const cards = [
    { title: labels.you, sub: labels.desktop, glyph: 'laptop' },
    { title: 'Mei', latin: true, sub: labels.teammate, avatar: 'M' },
    {
      title: 'release-assistant',
      latin: true,
      sub: labels.agent,
      glyph: 'robot'
    },
    {
      title: labels.website,
      sub: 'connect.dmsg.net',
      host: true,
      glyph: 'globe'
    }
  ]
    .map((card, i) => {
      const tile = rtl ? asker.w - 10 - tileSize : 10
      const tx = rtl ? 12 : textX
      const mid = asker.h / 2
      return `<g class="art-asker${i === 2 ? ' art-asker-agent' : ''}" data-asker="${ASKERS[i]}" transform="translate(${asker.x} ${asker.ys[i]})"><rect class="art-card" width="${asker.w}" height="${asker.h}" rx="11"/><rect class="art-tile" x="${tile}" y="10" width="${tileSize}" height="${tileSize}" rx="9"/>${
        card.avatar
          ? `<text class="art-avatar" x="${tile + tileSize / 2}" y="${mid + 5}" text-anchor="middle">${card.avatar}</text>`
          : icon(
              card.glyph,
              tile + (tileSize - 20) / 2,
              mid - 10,
              20,
              'art-icon'
            )
      }${text(card.title, tx, mid - 2, textW, 13.5, 'art-title', card.latin)}${text(card.sub, tx, mid + 14, textW, card.host ? 10.5 : 11.5, card.host ? 'art-host-name' : 'art-sub', card.host)}${check(rtl ? 10 : asker.w - 18)}</g>`
    })
    .join('')

  // Protocol names and hosts are not translated.
  const hosts = HOST_CARDS.map((card, i) => {
    const frame = `<g class="art-host" data-host="${HOSTS[i]}" transform="translate(${host.xs[i]} ${host.ys[i]})"><rect class="art-card" width="${host.w}" height="${host.h}" rx="10"/>`
    const body = narrow
      ? `${icon(card.glyph, 10, 9, 18, 'art-icon')}<text class="art-title" x="10" y="42" font-size="${fit(card.title, host.w - 20, 12)}">${card.title}</text>`
      : `${icon(card.glyph, 12, 16, 20, 'art-icon')}<text class="art-title" x="42" y="23" font-size="13">${card.title}</text><text class="art-host-name" x="42" y="39">${card.host}</text>`
    return `${frame}${body}${check(host.w - 18)}</g>`
  }).join('')

  const trail = (
    /** @type {string} */ name,
    /** @type {string} */ d,
    cls = ''
  ) =>
    live
      ? `<path class="art-trail${cls}" data-trail="${name}" d="${d}" pathLength="1"/>`
      : ''
  const traces = [
    ...g.routes.askers.map(
      (d, i) =>
        `<path class="art-route" id="${id}-ask-${i}" d="${d}"/>${trail(`ask-${i}`, d)}`
    ),
    ...g.routes.hosts.map(
      (d, i) =>
        `<path class="art-route art-route-out" id="${id}-host-${i}" d="${d}"/>${trail(`host-${i}`, d)}`
    )
  ].join('')
  const corridors = g.routes.corridors
    .map(
      (d, i) =>
        `<path class="art-corridor" id="${id}-room-${i}" d="${d}"/>${trail(`room-${i}`, d, ' art-trail-in')}`
    )
    .join('')
  const half = g.room.size / 2
  const roomTiles = g.rooms
    .map(
      (room) =>
        `<g class="art-room" data-room="${room.id}" transform="translate(${room.x - half} ${room.y - half})"><rect width="${g.room.size}" height="${g.room.size}" rx="${g.room.size / 4}"/>${icon(ROOM_GLYPH[room.id], (g.room.size - 18) / 2, (g.room.size - 18) / 2, 18)}</g>`
    )
    .join('')

  const packet = (/** @type {string} */ layer) =>
    live
      ? `<g class="art-packet" data-packet="${layer}"><circle class="art-packet-halo" r="10"/><circle class="art-packet-dot" r="4.3"/></g>`
      : ''
  const top = g.top
  const bottom = g.top + g.height
  const corner = (
    /** @type {number} */ x,
    /** @type {number} */ y,
    /** @type {number} */ dx,
    /** @type {number} */ dy
  ) => `M${x} ${y + dy}V${y}H${x + dx}`
  const crop = `${corner(2, top + 6, 12, 12)}${corner(g.width - 2, top + 6, -12, 12)}${corner(2, bottom - 6, 12, -12)}${corner(g.width - 2, bottom - 6, -12, -12)}`
  const label = (
    /** @type {{ x: number, y: number, w: number }} */ at,
    /** @type {string} */ value
  ) => text(value, at.x, at.y, at.w, 12.5, 'art-label')
  const area = `x="0" y="${top}" width="${g.width}" height="${g.height}"`

  return `<svg xmlns="http://www.w3.org/2000/svg" class="gate-art${narrow ? ' narrow' : ''}" direction="ltr" viewBox="0 ${top} ${g.width} ${g.height}" width="${g.width}" height="${g.height}" aria-hidden="true" focusable="false">
<defs>
<pattern id="${id}-dots" width="20" height="20" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r=".75" fill="#a9bdac"/></pattern>
<radialGradient id="${id}-fade" cx=".5" cy=".5" r=".6"><stop offset=".55" stop-color="#fff"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></radialGradient>
<mask id="${id}-grid" maskUnits="userSpaceOnUse" ${area}><rect ${area} fill="url(#${id}-fade)"/></mask>
<mask id="${id}-cut" maskUnits="userSpaceOnUse" x="0" y="0" width="42" height="48"><rect width="42" height="48" fill="#fff"/><path d="${GEM}" stroke="#000" stroke-width="5" stroke-linejoin="round"/></mask>
</defs>
<style>
.gate-art{font-family:Inter,-apple-system,BlinkMacSystemFont,'Segoe UI','PingFang SC','Microsoft YaHei',sans-serif}
.gate-art .art-card{fill:#fff;stroke:#cbd9cc}
.gate-art .art-tile{fill:#eaf1e9}
.gate-art .art-icon{fill:#145c45}
.gate-art .art-title{fill:#10251f;font-weight:600}
.gate-art .art-sub{fill:#5a6f63}
.gate-art .art-avatar{fill:#145c45;font-size:15px;font-weight:700}
.gate-art .art-asker-agent .art-card{fill:#10251f;stroke:#10251f}
.gate-art .art-asker-agent .art-tile{fill:#24443a}
.gate-art .art-asker-agent .art-icon{fill:#bfe0cb}
.gate-art .art-asker-agent .art-title{fill:#f2f6f0}
.gate-art .art-asker-agent .art-sub{fill:#a9c7b4}
.gate-art .art-host-name{fill:#5a6f63;font-family:ui-monospace,'SF Mono',SFMono-Regular,Menlo,Consolas,monospace;font-size:10.5px}
.gate-art .art-label{fill:#4e6257;font-weight:600}
.gate-art .art-route{fill:none;stroke:#b4c8b8;stroke-width:1.3}
.gate-art .art-route-out{stroke-dasharray:4 4}
.gate-art .art-corridor{fill:none;stroke:#5c9c80;stroke-width:1;stroke-dasharray:2 3}
.gate-art .art-room rect{fill:#1d6a50;stroke:#3d8467}
.gate-art .art-room path{fill:#cfe5d6}
.gate-art .art-check{fill:none;stroke:#145c45;stroke-width:1.8;stroke-linecap:round;stroke-linejoin:round;opacity:0}
.gate-art .art-asker-agent .art-check{stroke:#a9d5bd}
.gate-art .art-halo{fill:none;stroke:#b45309;stroke-width:1.4;opacity:${waiting ? 1 : 0}}
.gate-art .art-timer{fill:none;stroke:#b45309;stroke-width:2.4;stroke-linecap:round;opacity:0}
.gate-art .art-crop{fill:none;stroke:#10251f;stroke-opacity:.35}
.gate-art .art-trail{fill:none;stroke:#145c45;stroke-width:2;stroke-linecap:round;stroke-dasharray:1 1;stroke-dashoffset:1;opacity:0}
.gate-art .art-trail-in{stroke:#a9d5bd}
.gate-art .art-packet{opacity:0}
.gate-art .art-packet-halo{fill:#145c45;opacity:.16}
.gate-art .art-packet-dot{fill:#145c45;stroke:#fff;stroke-width:1.6}
.gate-art [data-packet=over] .art-packet-halo{fill:#a9d5bd;opacity:.3}
.gate-art [data-packet=over] .art-packet-dot{fill:#a9d5bd;stroke:#10251f}
.gate-art .art-request{fill:#b45309;stroke:#fff;stroke-width:1.6;opacity:0}
.gate-art .art-pass{fill:none;stroke:#b45309;stroke-width:1.5;opacity:0;transform-box:fill-box;transform-origin:center}
</style>
<rect ${area} fill="url(#${id}-dots)" mask="url(#${id}-grid)"/>
${narrow ? '' : `<path class="art-crop" d="${crop}"/>`}
${label(g.labels.people, labels.people)}
<text class="art-label" x="${g.labels.space.x}" y="${g.labels.space.y}" text-anchor="middle" direction="${rtl ? 'rtl' : 'ltr'}" font-size="${fit(labels.space, g.labels.space.w, 12.5)}">${esc(labels.space)}</text>
${label(g.labels.network, labels.network)}
${traces}
${packet('under')}
${live ? `<path class="art-request" d="M0 -7.5 7.5 0 0 7.5 -7.5 0Z"/>` : ''}
<g transform="translate(${g.mx} ${g.my}) scale(${g.s})"><g mask="url(#${id}-cut)" fill="#145c45"><path d="${POST}"/><path d="${DOME}"/></g><path d="${GEM}" fill="#b45309"/></g>
<circle class="art-halo" cx="${g.gapX}" cy="${g.gemY}" r="${(40 * g.s) / 4.6}"/>
<circle class="art-timer" cx="${g.gapX}" cy="${g.gemY}" r="${(46 * g.s) / 4.6}" pathLength="1" stroke-dasharray="1 1" transform="rotate(-90 ${g.gapX} ${g.gemY})"/>
${live ? `<circle class="art-pass" cx="${g.gapX}" cy="${g.gemY}" r="${(22 * g.s) / 4.6}"/>` : ''}
${corridors}
${roomTiles}
${packet('over')}
${cards}
${hosts}
</svg>`
}
