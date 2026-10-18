<script lang="ts">
  import { onMount } from 'svelte'
  import { locale, localeDir, t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import {
    frameLoop,
    prefersReducedMotion,
    splitStop,
    watchVisibility
  } from './motion'

  type Pt = { x: number; y: number }
  // A path sampled finely, so a packet keeps a steady speed along it.
  type Track = { pts: Pt[]; cum: number[]; len: number }
  type Kind =
    | 'read'
    | 'save'
    | 'post'
    | 'message'
    | 'add'
    | 'pay'
    | 'signin'
    | 'sign'
    | 'reply'
    | 'letter'
    | 'share'
    | 'delegate'
  type Req = { asker: number; kind: Kind; object: string }
  type Status =
    | 'sending'
    | 'waiting'
    | 'allowed'
    | 'encrypted'
    | 'saved'
    | 'sent'
    | 'approved'
    | 'rejected'
    | 'expired'
  type Packet = {
    id: number
    req: Req
    formal: boolean
    // In along the asker's lane, into a room or out to a host, back out, or
    // fading in place.
    stage: 'in' | 'room' | 'out' | 'back' | 'fade' | 'done'
    d: number
    alpha: number
    approved: boolean
    parked: boolean
    arrived: boolean
  }

  // The gate is the mark (static/_assets/dmsg/mark.svg) on its 42 × 48 grid:
  // a post and a dome, and between them the request that waits for you.
  const POST =
    'M4 0h4.5A1.5 1.5 0 0 1 10 1.5v45A1.5 1.5 0 0 1 8.5 48H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4Z'
  const DOME =
    'M15.5 0H18a24 24 0 0 1 0 48h-2.5a1.5 1.5 0 0 1-1.5-1.5v-45A1.5 1.5 0 0 1 15.5 0Z'
  const GEM = 'M12 17.5 18.5 24 12 30.5 5.5 24Z'
  // Rooms in the dome, in grid units: they clear the gem's cut-out and keep
  // inside the dome's curve. The agent's Agent ID sits by the gem: what it
  // signs leaves through the gap.
  const ROOMS: { glyph: GlyphName; x: number; y: number; w: number }[] = [
    { glyph: 'vault', x: 15.5, y: 6.9, w: 19 },
    { glyph: 'coins', x: 19, y: 14.2, w: 19.5 },
    { glyph: 'fingerprint', x: 22.5, y: 21.5, w: 18 },
    { glyph: 'chat', x: 19, y: 28.8, w: 19.5 },
    { glyph: 'folder', x: 15.5, y: 36.1, w: 19 }
  ]
  const AGENT_ROOM = 2
  // Out in the open: the production Agent Protocols hosts of `dmsg`
  // (src/dmsg_cli/production.toml), where the agent signs as its Agent ID.
  const HOSTS: { glyph: GlyphName; name: string; host: string }[] = [
    { glyph: 'discuss', name: 'Agent Discourse', host: 'discourse.dmsg.net' },
    { glyph: 'mail', name: 'Agent Mail', host: 'mail.dmsg.net' },
    { glyph: 'book', name: 'Agent Knowledge', host: 'knowledge.dmsg.net' },
    { glyph: 'link', name: 'Agent Delegation', host: 'api.dmsg.net' }
  ]
  const PROTOCOLS = 'Agent Protocols'
  const AGENT_ID = 'Agent ID'
  const SPECS = 'https://github.com/ldclabs/agent-protocols'
  // Sample data; names and files are identifiers, not copy.
  const AGENT = 'claude'
  const SITE = 'tokenlist.ing'
  const MEI = 'Mei'
  const CLI = 'dmsg'
  const NOTES = 'notes-v1.4.md'
  const BUILD = 'build-1.4.tar.gz'
  const RELEASE = '#release'
  const DESIGN = '#design'
  const AMOUNT = '0.4 BTC'
  // A non-breaking hyphen keeps the room's name on one line.
  const ROOM = 'v1.4‑qa'
  const TOPIC = 'caching'
  const ASKERS: { glyph: GlyphName | null; id: string }[] = [
    { glyph: 'robot', id: AGENT },
    { glyph: 'globe', id: SITE },
    { glyph: null, id: MEI },
    { glyph: 'terminal', id: CLI }
  ]
  const req = (asker: number, kind: Kind, object = ''): Req => ({
    asker,
    kind,
    object
  })
  const READ = req(0, 'read')
  const SAVE = req(0, 'save', NOTES)
  const PAY = req(0, 'pay', AMOUNT)
  const POST_RELEASE = req(0, 'post', RELEASE)
  const REPLY = req(0, 'reply', ROOM)
  const LETTER = req(0, 'letter', MEI)
  const SHARE = req(0, 'share', TOPIC)
  const SIGN_IN = req(1, 'signin', SITE)
  const MESSAGE = req(2, 'message', RELEASE)
  const MESSAGE_DESIGN = req(2, 'message', DESIGN)
  const ADD = req(3, 'add', BUILD)
  const DELEGATE = req(3, 'delegate', HOSTS[0]!.host)
  const SIGN = req(3, 'sign', BUILD)
  // What each asker sends when tapped, in turn.
  const SCRIPTS: Req[][] = [
    [PAY, REPLY, READ, LETTER, SAVE, SHARE, POST_RELEASE],
    [SIGN_IN],
    [MESSAGE, MESSAGE_DESIGN],
    [ADD, DELEGATE, SIGN]
  ]
  // What happens while nobody taps.
  const ROUTINE = [
    MESSAGE,
    REPLY,
    READ,
    ADD,
    LETTER,
    SAVE,
    MESSAGE_DESIGN,
    SHARE,
    POST_RELEASE
  ]
  const FORMAL = [DELEGATE, PAY, SIGN_IN, SIGN]
  const ROOM_OF: Record<Kind, number> = {
    read: 0,
    pay: 1,
    post: 3,
    message: 3,
    save: 4,
    add: 4,
    signin: -1,
    sign: -1,
    reply: -1,
    letter: -1,
    share: -1,
    delegate: -1
  }
  // What leaves through the gap, and the host it goes to.
  const HOST_OF: Partial<Record<Kind, number>> = {
    reply: 0,
    letter: 1,
    share: 2,
    delegate: 3
  }
  // How each asker's everyday work passes: the agent's grant, end-to-end
  // encryption, or your own command. The website only asks for approvals.
  const PASSED_AS: Status[] = ['allowed', 'approved', 'encrypted', 'saved']
  const WAIT_SECONDS = 15
  const INK = '16, 37, 31'
  const FOREST = '20, 92, 69'
  const MINT = '169, 213, 189'
  const AMBER = '180, 83, 9'
  const GREY = '78, 98, 87'
  const WIRE = '#9cb9ab'

  const uid = $props.id()
  const cut = `${uid}-cut`
  const ltr = (text: string) =>
    String.fromCharCode(0x2066) + text + String.fromCharCode(0x2069)
  const isFormal = (r: Req) =>
    r.kind === 'pay' ||
    r.kind === 'signin' ||
    r.kind === 'sign' ||
    r.kind === 'delegate'
  const hostOf = (r: Req) => HOST_OF[r.kind] ?? -1
  const pad = (n: number) => String(n).padStart(2, '0')
  // Everything each card can say. It all sits in one cell, hidden but for
  // what is current, so a card keeps its height as its text changes.
  const CAN_SAY = SCRIPTS.map((script, i) => {
    const said = new Set<Status>(['sending', PASSED_AS[i] ?? 'allowed'])
    for (const r of script) {
      if (isFormal(r))
        for (const s of ['waiting', 'approved', 'rejected', 'expired'] as const)
          said.add(s)
      else if (hostOf(r) >= 0) said.add('sent')
    }
    return [...said]
  })
  const HOST_SAYS: (Req | null)[][] = [
    [REPLY],
    [LETTER],
    [SHARE],
    [null, DELEGATE]
  ]

  const dir = $derived(localeDir($locale))
  const space = $derived(splitStop($t('Your space.'))[0])
  const say = $derived(splitStop($t('Your say.'))[0])
  const roles = $derived([
    $t('AI agent'),
    $t('Website'),
    $t('Teammate'),
    $t('Terminal')
  ])
  const roomNames = $derived([
    $t('Vault'),
    $t('Wallets'),
    AGENT_ID,
    $t('Messages'),
    $t('Files')
  ])
  const statusText = $derived<Record<Status, string>>({
    sending: $t('On its way'),
    waiting: $t('Waiting for you'),
    allowed: $t('Allowed by its grant'),
    encrypted: $t('End-to-end encrypted'),
    saved: $t('Encrypted and saved'),
    sent: $t('Sent under its own Agent ID'),
    approved: $t('Approved by you'),
    rejected: $t('Rejected'),
    expired: $t('Expired')
  })

  const hostNotes = $derived([
    $t('Its own signature, never yours'),
    $t('Only {name}’s agent can open it', { name: MEI }),
    $t('Signed, its source on record'),
    $t('Public to check, yours to revoke')
  ])

  let stage: HTMLDivElement
  let back: HTMLCanvasElement
  let front: HTMLCanvasElement
  let mark: HTMLDivElement
  let ticket: HTMLDivElement
  let list: HTMLDivElement
  let hostList: HTMLOListElement
  const cardEls: HTMLButtonElement[] = []
  const roomEls: HTMLElement[] = []
  const hostEls: HTMLElement[] = []

  let cards = $state<{ req: Req; status: Status }[]>([
    { req: SAVE, status: 'allowed' },
    { req: SIGN_IN, status: 'approved' },
    { req: MESSAGE, status: 'encrypted' },
    { req: ADD, status: 'saved' }
  ])
  let touched = $state(['', '', '', RELEASE, NOTES])
  let hits = $state([false, false, false, false, false])
  // The latest thing each host received from this computer.
  let wire = $state<(Req | null)[]>([REPLY, LETTER, SHARE, null])
  let reached = $state([false, false, false, false])
  let arrivals = $state([0, 0, 0, 0])
  let head = $state<{
    id: number
    req: Req
    left: number
    share: number
    more: number
  } | null>(null)
  let outcome = $state<'' | 'approved' | 'rejected' | 'expired'>('')
  let announce = $state('')
  let lit = $state(false)
  let hover = $state(-1)
  let pinged = $state(-1)

  const roomMeta = $derived(
    touched.map(
      (object, i) =>
        (object && ltr(object)) ||
        [$t('Deploy key'), $t('Team treasury'), AGENT][i] ||
        ''
    )
  )

  // The approval window always lays out a request, the current one or,
  // hidden, the first, so it keeps one size whatever it shows.
  const sheet = $derived(head?.req ?? FORMAL[0]!)

  const outcomeText = $derived(
    {
      '': '',
      approved: $t('Approved. Only this request passes.'),
      rejected: $t('Rejected. Nothing was signed or paid.'),
      expired: $t('Expired. Nothing was signed or paid.')
    }[outcome]
  )

  function ask(r: Req): string {
    const object = ltr(r.object)
    switch (r.kind) {
      case 'read':
        return $t('Read the deploy key')
      case 'save':
        return $t('Save {file}', { file: object })
      case 'post':
        return $t('Post in {channel}', { channel: object })
      case 'message':
        return $t('Message in {channel}', { channel: object })
      case 'add':
        return $t('Add {file}', { file: object })
      case 'pay':
        return $t('Pay {amount} to {payee}', {
          amount: object,
          payee: $t('Auditor')
        })
      case 'signin':
        return $t('Sign in with dMsg')
      case 'sign':
        return $t('Sign {file}', { file: object })
      case 'reply':
        return $t('Reply in {room}', { room: object })
      case 'letter':
        return $t('Write to {name}’s agent', { name: object })
      case 'share':
        return $t('Share a note on {topic}', { topic: object })
      case 'delegate':
        return $t('Let {agent} act for you', { agent: ltr(AGENT) })
    }
  }

  // What a host shows once a request reached it.
  function delivered(r: Req): string {
    const object = ltr(r.object)
    switch (r.kind) {
      case 'reply':
        return $t('Replied in {room}', { room: object })
      case 'letter':
        return $t('Sealed a letter to {name}’s agent', { name: object })
      case 'share':
        return $t('Shared a note on {topic}', { topic: object })
      default:
        return $t('{agent} acts for you · 30 days', { agent: ltr(AGENT) })
    }
  }

  function kindLabel(r: Req): string {
    if (r.kind === 'pay') return $t('Payment')
    if (r.kind === 'delegate') return $t('Delegation')
    return r.kind === 'signin' ? $t('Sign-in') : $t('Signature')
  }

  // What approving does, said before the details.
  function consequence(r: Req): string {
    if (r.kind === 'pay')
      return $t(
        'From Team treasury. It pays only after enough members approve.'
      )
    if (r.kind === 'signin')
      return $t('{site} receives a signed sign-in for this account.', {
        site: ltr(SITE)
      })
    if (r.kind === 'delegate')
      return $t(
        '{agent} may act for you on {host} for 30 days. Anyone can check it, and you can revoke it.',
        { agent: ltr(AGENT), host: ltr(r.object) }
      )
    return $t('The terminal receives your signature for this file.')
  }

  // The simulation lives outside Svelte's reactivity; these are set on mount.
  let decide: (approve: boolean) => void = () => {}
  let send: (asker: number) => void = () => {}
  let redraw = () => {}

  $effect(() => {
    void hover
    redraw()
  })

  function at(curve: [Pt, Pt, Pt, Pt], t: number): Pt {
    const [a, b, c, d] = curve
    const m = 1 - t
    const k0 = m * m * m
    const k1 = 3 * m * m * t
    const k2 = 3 * m * t * t
    const k3 = t * t * t
    return {
      x: k0 * a.x + k1 * b.x + k2 * c.x + k3 * d.x,
      y: k0 * a.y + k1 * b.y + k2 * c.y + k3 * d.y
    }
  }

  const cubic = (a: Pt, b: Pt, c: Pt, d: Pt): Pt[] =>
    Array.from({ length: 33 }, (_, i) => at([a, b, c, d], i / 32))

  // A polyline whose corners are rounded with radius r.
  function bend(corners: Pt[], r: number): Pt[] {
    const pts = corners.filter(
      (p, i) =>
        !i || Math.hypot(p.x - corners[i - 1]!.x, p.y - corners[i - 1]!.y) > 0.5
    )
    const out = [pts[0]!]
    for (let i = 1; i < pts.length - 1; i++) {
      const a = pts[i - 1]!
      const b = pts[i]!
      const c = pts[i + 1]!
      const ab = Math.hypot(a.x - b.x, a.y - b.y)
      const bc = Math.hypot(c.x - b.x, c.y - b.y)
      const k = Math.min(r, ab / 2, bc / 2)
      const p = {
        x: b.x + ((a.x - b.x) * k) / ab,
        y: b.y + ((a.y - b.y) * k) / ab
      }
      const q = {
        x: b.x + ((c.x - b.x) * k) / bc,
        y: b.y + ((c.y - b.y) * k) / bc
      }
      for (let j = 0; j <= 8; j++) {
        const t = j / 8
        const m = 1 - t
        out.push({
          x: m * m * p.x + 2 * m * t * b.x + t * t * q.x,
          y: m * m * p.y + 2 * m * t * b.y + t * t * q.y
        })
      }
    }
    out.push(pts[pts.length - 1]!)
    return out
  }

  function track(pts: Pt[]): Track {
    const cum = [0]
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1]!
      const b = pts[i]!
      cum.push(cum[i - 1]! + Math.hypot(b.x - a.x, b.y - a.y))
    }
    return { pts, cum, len: cum[cum.length - 1]! }
  }

  function pointAt({ pts, cum, len }: Track, d: number): Pt {
    if (d <= 0) return pts[0]!
    if (d >= len) return pts[pts.length - 1]!
    let i = 1
    while (cum[i]! < d) i++
    const a = pts[i - 1]!
    const b = pts[i]!
    const k = (d - cum[i - 1]!) / (cum[i]! - cum[i - 1]! || 1)
    return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k }
  }

  onMount(() => {
    const bctx = back.getContext('2d')!
    const fctx = front.getContext('2d')!
    const still = prefersReducedMotion()
    const none = (): Track => track([{ x: 0, y: 0 }])
    let lanes: Track[] = ASKERS.map(none)
    let toRoom: Track[] = ROOMS.map(none)
    let toHost: Track[] = HOSTS.map(none)
    let ports: Pt[] = []
    let check: Pt = { x: 0, y: 0 }
    let s = 1
    let box = { x: 0, y: 0, w: 0, h: 0 }
    let width = 0
    let height = 0
    let dots: Pt[] = []
    let pointer: Pt | null = null
    let packets: Packet[] = []
    let queue: Packet[] = []
    let clock = 0
    let deadline = 0
    let nextRoutine = 1.3
    let nextFormal = 0.6
    let seq = 0
    let routineAt = 0
    let formalAt = 0
    let visible = false
    // Each asker's next request when tapped: the agent asks to pay first,
    // and the terminal, whose delegation waits first, asks for a signature.
    const turn = [0, 0, 1, 2]
    // When each asker's card last showed a decision, so it stays readable.
    const decided = [-9, -9, -9, -9]
    let outcomeTimer: ReturnType<typeof setTimeout> | undefined
    const hitTimers: (ReturnType<typeof setTimeout> | undefined)[] = []
    const reachTimers: (ReturnType<typeof setTimeout> | undefined)[] = []

    function layout() {
      const frame = stage.getBoundingClientRect()
      const rel = (el: Element) => {
        const r = el.getBoundingClientRect()
        return {
          x: r.left - frame.left,
          y: r.top - frame.top,
          w: r.width,
          h: r.height
        }
      }
      width = frame.width
      height = frame.height
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      for (const [canvas, ctx] of [
        [back, bctx],
        [front, fctx]
      ] as const) {
        canvas.width = Math.round(width * dpr)
        canvas.height = Math.round(height * dpr)
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      }
      box = rel(mark)
      s = box.h / 48
      const sx = box.x + 12 * s
      const top = box.y
      const bottom = box.y + 48 * s
      check = { x: sx, y: box.y + 24 * s }
      const all = rel(list)
      const stacked = all.y + all.h <= top
      if (stacked) {
        // Askers stacked above the gate share one path into the gap.
        const start = { x: sx, y: all.y + all.h + 6 }
        const trunk = track([start, { x: sx, y: top }, check])
        lanes = ASKERS.map(() => trunk)
        ports = [start]
      } else {
        // Askers beside the gate: the upper ones come over the post into
        // the gap, the lower ones under it.
        ports = cardEls.map((el) => {
          const r = rel(el)
          return { x: r.x + r.w + 8, y: r.y + r.h / 2 }
        })
        lanes = ports.map((p) => {
          const down = p.y > check.y
          const edge = down ? bottom : top
          const q = { x: box.x + 2 * s, y: edge + (down ? 22 : -22) }
          const run = q.x - p.x
          return track([
            ...cubic(
              p,
              { x: p.x + run * 0.55, y: p.y },
              { x: q.x - run * 0.5, y: q.y },
              q
            ),
            ...cubic(
              q,
              { x: sx, y: q.y },
              { x: sx, y: q.y },
              { x: sx, y: edge }
            ).slice(1),
            check
          ])
        })
      }
      toRoom = roomEls.map((el) => {
        const r = rel(el)
        const c = { x: r.x + r.w / 2, y: r.y + r.h / 2 }
        return track(
          cubic(
            check,
            { x: check.x + 6 * s, y: check.y },
            { x: c.x - 4 * s, y: c.y },
            c
          )
        )
      })
      // Out of the gap's lower end: down onto a bus over a row of hosts, or
      // over to a rail beside a column of them. Below a stacked gate the
      // approval window comes first, so the wire turns at once and runs
      // down the lane the layout leaves at the left.
      const hosts = hostEls.map(rel)
      const busY = Math.min(...hosts.map((h) => h.y)) - 10
      const rail = Math.min(...hosts.map((h) => h.x)) - 14
      const turn = stacked ? bottom + 10 : busY
      const oneRow = hosts.every((h) => Math.abs(h.y - hosts[0]!.y) < 2)
      toHost = hosts.map((h) => {
        if (oneRow) {
          const cx = h.x + h.w / 2
          return track(
            bend(
              [
                check,
                { x: sx, y: busY },
                { x: cx, y: busY },
                { x: cx, y: h.y - 4 }
              ],
              12
            )
          )
        }
        const y = h.y + h.h / 2
        return track(
          bend(
            [
              check,
              { x: sx, y: turn },
              { x: rail, y: turn },
              { x: rail, y },
              { x: h.x - 4, y }
            ],
            12
          )
        )
      })
      dots = []
      const gap = 22
      for (let y = gap / 2; y < height; y += gap) {
        for (let x = gap / 2; x < width; x += gap) {
          const inside =
            x > box.x - 6 &&
            x < box.x + box.w + 6 &&
            y > box.y - 6 &&
            y < box.y + box.h + 6
          if (!inside) dots.push({ x, y })
        }
      }
    }

    const lane = (p: Packet) => lanes[p.req.asker] ?? lanes[0]!
    const room = (p: Packet) => toRoom[ROOM_OF[p.req.kind]] ?? toRoom[0]!
    const host = (p: Packet) => toHost[hostOf(p.req)] ?? toHost[0]!

    function position(p: Packet): Pt {
      if (p.stage === 'room') return pointAt(room(p), p.d)
      return p.stage === 'out' ? pointAt(host(p), p.d) : pointAt(lane(p), p.d)
    }

    // The front request waits in the gem; the next ones wait in the gap.
    function goal(p: Packet): number {
      const k = queue.indexOf(p)
      const len = lane(p).len
      return k <= 0 ? len : len - 9.5 * s - 8 - 14 * (k - 1)
    }

    function hit(index: number, object: string) {
      if (index < 0) return
      hits[index] = true
      if (index > AGENT_ROOM) touched[index] = object
      clearTimeout(hitTimers[index])
      hitTimers[index] = setTimeout(() => (hits[index] = false), 1100)
    }

    function reach(r: Req) {
      const index = hostOf(r)
      wire[index] = r
      reached[index] = true
      arrivals[index] = (arrivals[index] ?? 0) + 1
      clearTimeout(reachTimers[index])
      reachTimers[index] = setTimeout(() => (reached[index] = false), 1100)
    }

    function pass(p: Packet) {
      const out = hostOf(p.req) >= 0
      if (out) reach(p.req)
      else hit(ROOM_OF[p.req.kind], p.req.object)
      // Without motion the Agent ID signs as the request lands.
      if (out && still) hit(AGENT_ROOM, '')
      const status = out ? 'sent' : (PASSED_AS[p.req.asker] ?? 'allowed')
      if (!p.formal) cards[p.req.asker] = { req: p.req, status }
    }

    function showHead() {
      const first = queue[0]
      if (!first?.arrived) {
        head = null
        return
      }
      const left = Math.max(0, deadline - clock)
      head = {
        id: first.id,
        req: first.req,
        left: Math.ceil(left),
        share: left / WAIT_SECONDS,
        more: queue.length - 1
      }
    }

    function arrive(p: Packet) {
      p.arrived = true
      lit = true
      deadline = clock + WAIT_SECONDS
      announce = ask(p.req)
      showHead()
    }

    function park(p: Packet) {
      p.parked = true
      cards[p.req.asker] = { req: p.req, status: 'waiting' }
      if (queue[0] === p) arrive(p)
    }

    function flash(kind: 'approved' | 'rejected' | 'expired') {
      outcome = kind
      clearTimeout(outcomeTimer)
      outcomeTimer = setTimeout(() => (outcome = ''), 2600)
    }

    function spawn(r: Req) {
      const formal = isFormal(r)
      const p: Packet = {
        id: ++seq,
        req: r,
        formal,
        stage: 'in',
        d: 0,
        alpha: 1,
        approved: false,
        parked: false,
        arrived: false
      }
      cards[r.asker] = { req: r, status: 'sending' }
      if (formal) queue.push(p)
      if (!still) {
        packets.push(p)
        return
      }
      // Without motion a request lands at once: work passes, and a request
      // that needs you is already waiting.
      if (!formal) {
        pass(p)
        return
      }
      packets.push(p)
      p.d = goal(p)
      park(p)
    }

    function pickRoutine(): Req | undefined {
      for (let k = 0; k < ROUTINE.length; k++) {
        const r = ROUTINE[routineAt++ % ROUTINE.length]!
        const busy = queue.some((p) => p.req.asker === r.asker)
        if (!busy && clock - (decided[r.asker] ?? 0) > 3) return r
      }
      return undefined
    }

    // A decision on the front request, or its expiry, moves the queue on.
    function settle() {
      lit = false
      // With motion, the next request moves up to the gem on its own.
      const next = queue[0]
      if (still && next) {
        next.d = goal(next)
        arrive(next)
      }
      if (still && !queue.length) spawn(FORMAL[formalAt++ % FORMAL.length]!)
      nextFormal = Math.max(nextFormal, 3.5)
      showHead()
    }

    decide = (approve: boolean) => {
      const p = queue[0]
      if (!p?.arrived) return
      queue.shift()
      const hadFocus = ticket.contains(document.activeElement)
      cards[p.req.asker] = {
        req: p.req,
        status: approve ? 'approved' : 'rejected'
      }
      decided[p.req.asker] = clock
      p.approved = approve
      if (approve && p.req.kind === 'pay') {
        if (still) hit(1, '')
        p.stage = 'room'
        p.d = 0
      } else if (approve && hostOf(p.req) >= 0) {
        // An approved delegation is signed and published.
        if (still) reach(p.req)
        p.stage = 'out'
        p.d = 0
      } else p.stage = 'back'
      if (still) packets = packets.filter((x) => x !== p)
      flash(approve ? 'approved' : 'rejected')
      const said = approve
        ? $t('Approved. Only this request passes.')
        : $t('Rejected. Nothing was signed or paid.')
      settle()
      // Without motion the next request is already waiting; say both.
      announce = head ? `${said} ${ask(head.req)}` : said
      if (hadFocus) ticket.focus()
      draw()
    }

    function expire() {
      const p = queue.shift()!
      p.stage = 'fade'
      cards[p.req.asker] = { req: p.req, status: 'expired' }
      decided[p.req.asker] = clock
      flash('expired')
      announce = $t('Expired. Nothing was signed or paid.')
      settle()
    }

    send = (asker: number) => {
      pinged = asker
      setTimeout(() => pinged === asker && (pinged = -1), 600)
      // One request per asker waits at a time.
      if (queue.some((p) => p.req.asker === asker)) return
      const script = SCRIPTS[asker] ?? []
      const r = script[turn[asker]! % script.length]
      if (!r || (isFormal(r) && queue.length >= 3)) return
      turn[asker]!++
      if (isFormal(r)) nextFormal = Math.max(nextFormal, 5)
      spawn(r)
      draw()
    }

    function advance(p: Packet, dt: number) {
      switch (p.stage) {
        case 'in': {
          const end = p.formal ? goal(p) : lane(p).len
          p.d = Math.min(end, p.d + dt * (p.formal ? 170 : 210))
          if (p.d < end) break
          if (!p.formal) {
            const out = hostOf(p.req) >= 0
            // Its Agent ID signs it as it passes the gem.
            if (out) hit(AGENT_ROOM, '')
            p.stage = out ? 'out' : 'room'
            p.d = 0
          } else if (!p.parked) park(p)
          else if (queue[0] === p && !p.arrived) arrive(p)
          break
        }
        case 'room':
          p.d += dt * 130
          if (p.d >= room(p).len) {
            if (p.formal) hit(1, '')
            else pass(p)
            p.stage = 'done'
          }
          break
        case 'out':
          p.d += dt * 230
          if (p.d >= host(p).len) {
            if (p.formal) reach(p.req)
            else pass(p)
            p.stage = 'done'
          }
          break
        case 'back': {
          const len = lane(p).len
          p.d -= dt * (p.approved ? 240 : 200)
          if (!p.approved) p.alpha = Math.max(0, (p.d / len - 0.35) / 0.65)
          if (p.d <= 0 || p.alpha <= 0) p.stage = 'done'
          break
        }
        case 'fade':
          p.alpha -= dt / 0.8
          if (p.alpha <= 0) p.stage = 'done'
          break
      }
    }

    function step(dt: number) {
      clock += dt
      nextRoutine -= dt
      if (nextRoutine <= 0) {
        const r = pickRoutine()
        if (r) spawn(r)
        nextRoutine = 2.1 + Math.random() * 1.4
      }
      if (!queue.length) {
        nextFormal -= dt
        if (nextFormal <= 0) {
          spawn(FORMAL[formalAt++ % FORMAL.length]!)
          nextFormal = 4 + Math.random() * 2.5
        }
      }
      for (const p of packets) advance(p, dt)
      const first = queue[0]
      if (first?.arrived && clock >= deadline) expire()
      else if (
        first?.arrived &&
        Math.floor(clock * 4) !== Math.floor((clock - dt) * 4)
      )
        showHead()
      packets = packets.filter((p) => p.stage !== 'done')
      draw()
    }

    function diamond(ctx: CanvasRenderingContext2D, p: Pt, size: number) {
      ctx.beginPath()
      ctx.moveTo(p.x, p.y - size)
      ctx.lineTo(p.x + size, p.y)
      ctx.lineTo(p.x, p.y + size)
      ctx.lineTo(p.x - size, p.y)
      ctx.closePath()
    }

    function drawBack() {
      const ctx = bctx
      ctx.clearRect(0, 0, width, height)

      // Graph-paper dots that lean away from the pointer.
      ctx.fillStyle = `rgba(${INK}, 0.12)`
      ctx.beginPath()
      const near: [Pt, number, number, number][] = []
      for (const d of dots) {
        if (pointer) {
          const dx = d.x - pointer.x
          const dy = d.y - pointer.y
          const dist = Math.hypot(dx, dy)
          if (dist < 120) {
            near.push([d, dx, dy, dist])
            continue
          }
        }
        ctx.rect(d.x, d.y, 1.3, 1.3)
      }
      ctx.fill()
      for (const [d, dx, dy, dist] of near) {
        const f = 1 - dist / 120
        const k = dist ? (f * f * 10) / dist : 0
        ctx.fillStyle = `rgba(${FOREST}, ${0.14 + f * 0.4})`
        ctx.fillRect(d.x + dx * k, d.y + dy * k, 1.5 + f, 1.5 + f)
      }

      // Each lane stops where the gem's cut-out begins.
      ctx.lineCap = 'round'
      ctx.lineJoin = 'round'
      const drawn = new Set<Track>()
      lanes.forEach((path, i) => {
        if (drawn.has(path) && hover !== i) return
        drawn.add(path)
        const stop = path.len - 9.2 * s
        ctx.beginPath()
        path.pts.forEach((p, k) => {
          if ((path.cum[k] ?? 0) > stop) return
          if (k) ctx.lineTo(p.x, p.y)
          else ctx.moveTo(p.x, p.y)
        })
        const hot = hover === i
        ctx.lineWidth = hot ? 1.6 : 1
        ctx.strokeStyle = hot ? `rgba(${FOREST}, 0.75)` : `rgba(${INK}, 0.22)`
        ctx.stroke()
      })
      // The way out starts below the gem; shared stretches draw over each
      // other in one opaque colour, so they don't darken.
      ctx.setLineDash([3, 5])
      ctx.lineWidth = 1.2
      ctx.strokeStyle = WIRE
      for (const path of toHost) {
        const from = 9.2 * s
        const start = pointAt(path, from)
        ctx.beginPath()
        ctx.moveTo(start.x, start.y)
        path.pts.forEach((p, k) => {
          if ((path.cum[k] ?? 0) > from) ctx.lineTo(p.x, p.y)
        })
        ctx.stroke()
      }
      ctx.setLineDash([])
      const ends = toHost.map((path) => path.pts[path.pts.length - 1]!)
      for (const p of [...ports, ...ends]) {
        ctx.beginPath()
        ctx.arc(p.x, p.y, 3, 0, Math.PI * 2)
        ctx.fillStyle = '#f7f8f4'
        ctx.fill()
        ctx.lineWidth = 1.2
        ctx.strokeStyle = `rgba(${INK}, 0.6)`
        ctx.stroke()
      }
    }

    function drawFront() {
      const ctx = fctx
      ctx.clearRect(0, 0, width, height)
      const first = queue[0]

      // The waiting gem echoes outward, the shape of the mark's cut-out.
      if (first?.arrived && !still) {
        for (const offset of [0, 0.5]) {
          const phase = (clock * 0.55 + offset) % 1
          diamond(ctx, check, (9.2 + phase * 2.6) * s)
          ctx.lineWidth = 1.5
          ctx.strokeStyle = `rgba(${AMBER}, ${0.5 * (1 - phase)})`
          ctx.stroke()
        }
      }

      for (const p of packets) {
        if (p.arrived && p === first) continue
        const q = position(p)
        if (!p.formal) {
          if (p.stage === 'room') {
            ctx.beginPath()
            ctx.arc(q.x, q.y, 6, 0, Math.PI * 2)
            ctx.fillStyle = `rgba(${MINT}, 0.22)`
            ctx.fill()
            ctx.beginPath()
            ctx.arc(q.x, q.y, 3, 0, Math.PI * 2)
            ctx.fillStyle = `rgba(${MINT}, 1)`
            ctx.fill()
            continue
          }
          const path = p.stage === 'out' ? host(p) : lane(p)
          for (let k = 1; k < 5; k++) {
            const t = pointAt(path, p.d - k * 5)
            ctx.beginPath()
            ctx.arc(t.x, t.y, 2.6 - k * 0.45, 0, Math.PI * 2)
            ctx.fillStyle = `rgba(${FOREST}, ${0.32 - k * 0.06})`
            ctx.fill()
          }
          // Past the gate it goes out signed, ringed like a seal.
          if (p.stage === 'out') {
            ctx.beginPath()
            ctx.arc(q.x, q.y, 6.5, 0, Math.PI * 2)
            ctx.fillStyle = '#f7f8f4'
            ctx.fill()
            ctx.lineWidth = 1.4
            ctx.strokeStyle = `rgba(${FOREST}, 0.7)`
            ctx.stroke()
          }
          ctx.beginPath()
          ctx.arc(q.x, q.y, 3, 0, Math.PI * 2)
          ctx.fillStyle = `rgba(${FOREST}, 0.95)`
          ctx.fill()
          continue
        }
        if (p.stage === 'room') {
          diamond(ctx, q, 6)
          ctx.fillStyle = `rgba(${MINT}, 1)`
          ctx.fill()
        } else if ((p.stage === 'back' && p.approved) || p.stage === 'out') {
          diamond(ctx, q, 6)
          ctx.fillStyle = `rgba(${FOREST}, 1)`
          ctx.fill()
        } else if (p.stage === 'back' || p.stage === 'fade') {
          diamond(ctx, q, 6)
          ctx.lineWidth = 1.4
          ctx.strokeStyle = `rgba(${GREY}, ${p.alpha})`
          ctx.stroke()
        } else if (p.parked) {
          diamond(ctx, q, 5.5)
          ctx.fillStyle = `rgba(${AMBER}, 0.6)`
          ctx.fill()
        } else {
          diamond(ctx, q, 6)
          ctx.fillStyle = '#f7f8f4'
          ctx.fill()
          ctx.lineWidth = 1.6
          ctx.strokeStyle = `rgba(${AMBER}, 0.95)`
          ctx.stroke()
        }
      }
    }

    function draw() {
      drawBack()
      drawFront()
    }
    redraw = draw

    function seedStill() {
      spawn(FORMAL[0]!)
      formalAt = 1
    }

    layout()
    const loop = frameLoop(step, () => visible && !still)
    const resize = new ResizeObserver(() => {
      layout()
      draw()
    })
    resize.observe(stage)
    resize.observe(mark)
    resize.observe(list)
    resize.observe(hostList)
    for (const el of [...cardEls, ...hostEls]) resize.observe(el)
    const stopWatch = watchVisibility(stage, (value) => {
      visible = value
      loop.wake()
    })
    const onMove = (event: PointerEvent) => {
      const rect = stage.getBoundingClientRect()
      pointer = { x: event.clientX - rect.left, y: event.clientY - rect.top }
      if (still) draw()
    }
    const onLeave = () => {
      pointer = null
      if (still) draw()
    }
    stage.addEventListener('pointermove', onMove)
    stage.addEventListener('pointerleave', onLeave)
    if (still) seedStill()
    draw()

    return () => {
      loop.stop()
      resize.disconnect()
      stopWatch()
      clearTimeout(outcomeTimer)
      for (const timer of [...hitTimers, ...reachTimers]) clearTimeout(timer)
      redraw = () => {}
      stage.removeEventListener('pointermove', onMove)
      stage.removeEventListener('pointerleave', onLeave)
    }
  })
</script>

{#snippet orb(asker: number)}
  {@const who = ASKERS[asker]}
  <span class="gh-orb" aria-hidden="true"
    >{#if who?.glyph}<Glyph name={who.glyph} size={18} />{:else}{who?.id.at(
        0
      )}{/if}</span
  >
{/snippet}

<figure class="fig gate-fig">
  <div class="gate-frame">
    <div class="gate-stage" bind:this={stage} dir="ltr">
      <canvas class="back" bind:this={back} aria-hidden="true"></canvas>

      <div class="gh-askers" {dir}>
        <p class="gh-label">{$t('Who’s asking')}</p>
        <div class="gh-list" bind:this={list}>
          {#each ASKERS as asker, i}
            {@const card = cards[i]}
            <button
              class="gh-asker"
              class:hot={hover === i}
              class:ping={pinged === i}
              class:waiting={card?.status === 'waiting'}
              bind:this={cardEls[i]}
              aria-label={$t('Send a request from {source}', {
                source: roles[i] ?? ''
              })}
              onpointerenter={() => (hover = i)}
              onpointerleave={() => (hover = -1)}
              onfocus={() => (hover = i)}
              onblur={() => (hover = -1)}
              onclick={() => send(i)}
            >
              {@render orb(i)}
              <span class="gh-who"
                ><strong>{roles[i]}</strong><code>{asker.id}</code></span
              >
              {#if card}
                <span class="gh-lines">
                  {#each SCRIPTS[i] ?? [] as r}
                    <span class="gh-req" aria-hidden="true"
                      ><i class:formal={isFormal(r)}></i>{ask(r)}</span
                    >
                  {/each}
                  <span class="gh-req"
                    ><i class:formal={isFormal(card.req)}></i>{ask(
                      card.req
                    )}</span
                  >
                </span>
                <span class="gh-lines">
                  {#each CAN_SAY[i] ?? [] as status}
                    <span class="gh-state {status}" aria-hidden="true"
                      >{statusText[status]}</span
                    >
                  {/each}
                  <span class="gh-state {card.status}"
                    >{statusText[card.status]}</span
                  >
                </span>
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <div class="gh-gate">
        <p class="gh-label">{space}</p>
        <div class="gh-mark" class:lit bind:this={mark}>
          <svg viewBox="0 0 42 48" aria-hidden="true" focusable="false">
            <mask
              id={cut}
              maskUnits="userSpaceOnUse"
              x="0"
              y="0"
              width="42"
              height="48"
            >
              <rect width="42" height="48" fill="#fff" />
              <path
                d={GEM}
                stroke="#000"
                stroke-width="5"
                stroke-linejoin="round"
              />
            </mask>
            <g class="gh-body" mask="url(#{cut})"
              ><path d={POST} /><path d={DOME} /></g
            >
            <path class="gh-socket" d={GEM} />
            <path class="gh-gem" d={GEM} />
          </svg>
          <ul class="gh-rooms">
            {#each ROOMS as room, i}
              <li
                class="gh-room"
                class:hit={hits[i]}
                style:--x={room.x}
                style:--y={room.y}
                style:--w={room.w}
              >
                <span class="gh-room-icon" bind:this={roomEls[i]}
                  ><Glyph name={room.glyph} size={16} /></span
                >
                <span class="gh-room-text" {dir}
                  ><strong>{roomNames[i]}</strong><small>{roomMeta[i]}</small
                  ></span
                >
              </li>
            {/each}
          </ul>
        </div>
        <p class="gh-sub">{$t('Encrypted on this computer')}</p>
      </div>

      <div class="gh-you" {dir}>
        <p class="gh-label">{say}</p>
        <div class="gh-say">
          <div
            class="approval-window gh-ticket"
            class:awaiting={head}
            bind:this={ticket}
            tabindex="-1"
            role="group"
            aria-label={$t('Approval')}
          >
            <div class="win-bar">
              <span class="lights" aria-hidden="true"
                ><i></i><i></i><i></i></span
              >
              <span class="win-title">{$t('dMsg · Approval')}</span>
            </div>
            <div class="gh-tk">
              {#key head?.id}
                <div class="gh-sheet" class:ghost={!head} inert={!head}>
                  <div class="gh-tk-top">
                    <span class="gh-kind"><i></i>{kindLabel(sheet)}</span>
                    {#if head?.more}
                      <span class="gh-more"
                        >{$t('{n} more waiting', { n: head.more })}</span
                      >
                    {/if}
                    <span class="gh-timer"
                      >0:{pad(head?.left ?? WAIT_SECONDS)}</span
                    >
                  </div>
                  <h4 class="gh-lines">
                    {#each FORMAL as r}
                      <span aria-hidden="true">{ask(r)}</span>
                    {/each}
                    <span>{ask(sheet)}</span>
                  </h4>
                  <p class="gh-from">
                    {@render orb(sheet.asker)}<span>{roles[sheet.asker]}</span
                    ><code>{ASKERS[sheet.asker]?.id}</code>
                  </p>
                  <p class="gh-effect gh-lines">
                    {#each FORMAL as r}
                      <span aria-hidden="true">{consequence(r)}</span>
                    {/each}
                    <span>{consequence(sheet)}</span>
                  </p>
                  <div class="gc-actions">
                    <button
                      class="button secondary"
                      onclick={() => decide(false)}>{$t('Reject')}</button
                    >
                    <button class="button primary" onclick={() => decide(true)}
                      >{$t('Approve')}</button
                    >
                  </div>
                </div>
              {/key}
              {#if head}
                <i class="gc-expiry" style:transform="scaleX({head.share})"></i>
              {:else if outcome}
                <p class="gh-outcome gh-over {outcome}">
                  <Glyph
                    name={outcome === 'approved'
                      ? 'check'
                      : outcome === 'rejected'
                        ? 'close'
                        : 'time'}
                    size={18}
                  />{outcomeText}
                </p>
              {:else}
                <p class="gc-idle gh-over">
                  {$t('Nothing is waiting. Tap a source to send a request.')}
                </p>
              {/if}
            </div>
          </div>
          <p class="gh-zero">
            <span>{$t('Signed, paid or shared without you')}</span><b>0</b>
          </p>
        </div>
      </div>

      <div class="gh-net">
        <p class="gh-label" {dir}>
          {$t('Out in the open')} ·
          <a href={SPECS} target="_blank" rel="noopener noreferrer"
            >{PROTOCOLS}<Glyph name="arrow-up-right" size={12} /></a
          >
        </p>
        <ol class="gh-hosts" bind:this={hostList}>
          {#each HOSTS as host, i}
            {@const last = wire[i]}
            <li
              class="gh-host"
              class:hit={reached[i]}
              bind:this={hostEls[i]}
              {dir}
            >
              <strong class="gh-host-name"
                ><Glyph name={host.glyph} size={16} />{host.name}</strong
              >
              <code>{host.host}</code>
              <span class="gh-lines">
                {#each HOST_SAYS[i] ?? [] as r}
                  <span class="gh-req" class:idle={!r} aria-hidden="true"
                    ><i class:formal={r && isFormal(r)}></i>{r
                      ? delivered(r)
                      : $t('Nothing published yet')}</span
                  >
                {/each}
                {#key arrivals[i]}
                  <span class="gh-req" class:idle={!last}
                    ><i class:formal={last && isFormal(last)}></i>{last
                      ? delivered(last)
                      : $t('Nothing published yet')}</span
                  >
                {/key}
              </span>
              <span class="gh-state">{hostNotes[i]}</span>
            </li>
          {/each}
        </ol>
      </div>

      <canvas class="front" bind:this={front} aria-hidden="true"></canvas>
    </div>
  </div>
  <p class="sr-only" aria-live="polite">{announce}</p>
  <p class="sr-only">
    {$t(
      'An AI agent, a website, a teammate and the terminal ask for things in your vault, wallets, messages and files. Work an agent was granted, messages that arrive encrypted and your own commands pass; a payment, a sign-in, a signature or a delegation waits at the gate until you approve it. The agent’s replies, letters and notes go out through the same gate to Agent Protocols hosts, signed with its own Agent ID.'
    )}
  </p>
  <figcaption>
    <span class="fig-no">{$t('Fig. {n}', { n: 1 })}</span>
    <span
      >{$t(
        'Everything asks. Only you approve. Your agents go out as themselves.'
      )}</span
    >
    <span class="fig-note"
      ><i class="live"></i>{$t('Simulation · nothing is signed')}</span
    >
  </figcaption>
</figure>
