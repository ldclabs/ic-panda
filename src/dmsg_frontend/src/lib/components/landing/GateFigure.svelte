<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import type { GlyphName } from './glyphs'
  import { frameLoop, prefersReducedMotion, watchVisibility } from './motion'

  type Pt = { x: number; y: number }
  type Curve = [Pt, Pt, Pt, Pt]
  type Stage =
    'in' | 'approach' | 'wait' | 'through' | 'out' | 'back' | 'fade' | 'done'
  // One lane per source: its path in, its path through the gate and where a
  // request that needs a person waits. Layout updates lanes in place.
  type Lane = { index: number; src: Curve; inner: Curve; wait: number }
  type Exit = { curve: Curve; length: number }
  type Packet = {
    id: number
    lane: Lane
    exit: Exit
    formal: boolean
    stage: Stage
    t: number
    alpha: number
    at: Pt
    cipher: string[]
  }

  // The approval model's own source tags; protocol text, not copy.
  const SOURCES: { glyph: GlyphName; tag: string; object: string }[] = [
    { glyph: 'laptop', tag: 'gui', object: 'MacBook Air' },
    { glyph: 'terminal', tag: 'cli', object: 'build-1.4.tar.gz' },
    {
      glyph: 'robot',
      tag: 'agent:did:agent:…',
      object: 'release-brief.txt · v1'
    },
    { glyph: 'globe', tag: 'connect:https://…', object: 'tokenlist.ing' }
  ]
  const DAEMON = 'dmsg daemon'
  const ICP = 'ICP'
  const HEX = '0123456789abcdef'
  const WAIT_SECONDS = 15
  const INK = '16, 37, 31'
  const FOREST = '20, 92, 69'
  const JADE = '45, 121, 96'
  const AMBER = '180, 83, 9'

  const names = $derived([
    $t('Desktop app'),
    $t('Terminal'),
    $t('AI agent'),
    $t('Website')
  ])
  const shortNames = $derived([
    $t('Desktop'),
    $t('Terminal'),
    $t('Agent'),
    $t('Website')
  ])
  const asks = $derived([
    $t('Approve a new device'),
    $t('Sign a file'),
    $t('Sign a statement'),
    $t('Sign in with dMsg')
  ])

  let stage: HTMLDivElement
  let canvas: HTMLCanvasElement
  let card: HTMLDivElement
  let vertical = $state(false)
  let anchors = $state<{ src: Pt[]; dst: Pt[]; core: Pt; r: number }>({
    src: [],
    dst: [],
    core: { x: 0, y: 0 },
    r: 0
  })
  let head = $state<{ src: number; left: number; share: number } | null>(null)
  let outcome = $state<'' | 'approved' | 'rejected' | 'expired'>('')
  let stats = $state({ seen: 0, approved: 0, rejected: 0, expired: 0 })
  let hover = $state(-1)
  let pinged = $state(-1)

  const waiting = $derived(head ? SOURCES[head.src] : undefined)
  const outcomeText = $derived(
    {
      '': '',
      approved: $t('Approved. The signature goes to ICP.'),
      rejected: $t('Rejected. Nothing was signed.'),
      expired: $t('Expired. Nothing was signed.')
    }[outcome]
  )

  // The simulation lives outside Svelte's reactivity; these are set on mount.
  let decide: (approve: boolean) => void = () => {}
  let send: (src: number) => void = () => {}

  function at([a, b, c, d]: Curve, t: number): Pt {
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

  function length(curve: Curve): number {
    let total = 0
    let prev = curve[0]
    for (let i = 1; i <= 32; i++) {
      const next = at(curve, i / 32)
      total += Math.hypot(next.x - prev.x, next.y - prev.y)
      prev = next
    }
    return total
  }

  const digit = () => HEX.charAt((Math.random() * 16) | 0)
  const pad = (n: number) => String(n).padStart(2, '0')

  onMount(() => {
    const ctx = canvas.getContext('2d')!
    const still = prefersReducedMotion()
    const origin = (): Curve => [
      { x: 0, y: 0 },
      { x: 0, y: 0 },
      { x: 0, y: 0 },
      { x: 0, y: 0 }
    ]
    const lanes: Lane[] = SOURCES.map((_, index) => ({
      index,
      src: origin(),
      inner: origin(),
      wait: 0
    }))
    const cloud: Exit = { curve: origin(), length: 1 }
    const icp: Exit = { curve: origin(), length: 1 }
    let width = 0
    let height = 0
    let gate: Pt[] = []
    let core: Pt[] = []
    let coreEdge: Pt[] = []
    let dots: Pt[] = []
    let pointer: Pt | null = null
    let packets: Packet[] = []
    let queue: Packet[] = []
    let seals: number[] = []
    let clock = 0
    let deadline = 0
    let nextSync = 0.4
    let nextFormal = 0.7
    let lastFormal = 1
    let seq = 0
    let shownId = -1
    let visible = false
    let outcomeTimer: ReturnType<typeof setTimeout> | undefined

    function layout() {
      const rect = stage.getBoundingClientRect()
      width = rect.width
      height = rect.height
      const dpr = Math.min(window.devicePixelRatio || 1, 2)
      canvas.width = Math.round(width * dpr)
      canvas.height = Math.round(height * dpr)
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
      const down = width < 720
      vertical = down
      // Work along the flow (u) and across it (v), then map to the screen.
      const U = down ? height : width
      const V = down ? width : height
      const map = (u: number, v: number): Pt =>
        down ? { x: v, y: u } : { x: u, y: v }
      const curve = (au: number, av: number, bu: number, bv: number): Curve => {
        const k = (bu - au) / 2
        return [map(au, av), map(au + k, av), map(bu - k, bv), map(bu, bv)]
      }
      const su = down ? 100 : U * 0.215
      const across = down
        ? [0.125, 0.375, 0.625, 0.875]
        : [0.14, 0.38, 0.62, 0.86]
      const r = down ? Math.min(V * 0.27, 96) : Math.min(V * 0.31, U * 0.105)
      const gu = U * (down ? 0.47 : 0.5) - r / 2
      const gv = V / 2
      const du = down ? U - 108 : U * 0.79
      const exitU = gu + r
      // Requests that need a person wait at the inner gate's flat edge.
      const edge = gu + r * 0.15
      for (const lane of lanes) {
        const sv = (across[lane.index] ?? 0.5) * V
        const ev = gv + (lane.index - 1.5) * r * 0.4
        lane.src = curve(su, sv, gu, ev)
        lane.inner = [
          map(gu, ev),
          map(gu + r * 0.5, ev),
          map(exitU - r * 0.5, gv),
          map(exitU, gv)
        ]
        let lo = 0
        let hi = 0.5
        for (let i = 0; i < 18; i++) {
          const mid = (lo + hi) / 2
          const p = at(lane.inner, mid)
          if ((down ? p.y : p.x) < edge) lo = mid
          else hi = mid
        }
        lane.wait = hi
      }
      cloud.curve = curve(exitU, gv, du, V * 0.3)
      icp.curve = curve(exitU, gv, du, V * 0.7)
      cloud.length = length(cloud.curve)
      icp.length = length(icp.curve)
      const arc = (cu: number, radius: number) =>
        Array.from({ length: 61 }, (_, k) => {
          const a = -Math.PI / 2 + (Math.PI * k) / 60
          return map(cu + radius * Math.cos(a), gv + radius * Math.sin(a))
        })
      gate = arc(gu, r)
      core = arc(edge, r * 0.78)
      coreEdge = [map(edge, gv - r * 0.78), map(edge, gv + r * 0.78)]
      dots = []
      const gap = 22
      for (let y = gap / 2; y < height; y += gap) {
        for (let x = gap / 2; x < width; x += gap) {
          const u = down ? y : x
          const v = down ? x : y
          const inside =
            u >= gu - 4 && (u - gu) ** 2 + (v - gv) ** 2 <= (r + 6) ** 2
          if (!inside) dots.push({ x, y })
        }
      }
      anchors = {
        src: across.map((f) => map(su, f * V)),
        dst: [map(du, V * 0.3), map(du, V * 0.7)],
        core: map(gu + r * (down ? 0.45 : 0.52), gv),
        r
      }
    }

    function position(p: Packet): Pt {
      switch (p.stage) {
        case 'in':
          return at(p.lane.src, p.t)
        case 'back':
          return p.t > 1 ? at(p.lane.inner, p.t - 1) : at(p.lane.src, p.t)
        case 'approach':
          return at(p.lane.inner, p.t * p.lane.wait)
        case 'wait':
          return at(p.lane.inner, p.lane.wait)
        case 'through':
          return at(p.lane.inner, p.t)
        case 'out':
          return at(p.exit.curve, p.t)
        default:
          return p.at
      }
    }

    function spawn(src: number, formal: boolean): Packet {
      const p: Packet = {
        id: ++seq,
        lane: lanes[src] ?? lanes[0]!,
        exit: cloud,
        formal,
        stage: 'in',
        t: 0,
        alpha: 1,
        at: { x: 0, y: 0 },
        cipher: Array.from({ length: 9 }, digit)
      }
      packets.push(p)
      stats.seen++
      return p
    }

    const pendingFormal = () =>
      packets.filter(
        (p) =>
          p.formal &&
          (p.stage === 'in' || p.stage === 'approach' || p.stage === 'wait')
      )

    function showHead() {
      const first = queue[0]
      if (!first) {
        head = null
        shownId = -1
        return
      }
      const left = Math.max(0, deadline - clock)
      head = {
        src: first.lane.index,
        left: Math.ceil(left),
        share: left / WAIT_SECONDS
      }
    }

    function promote() {
      deadline = clock + WAIT_SECONDS
      shownId = queue[0]?.id ?? -1
      showHead()
    }

    function flash(kind: 'approved' | 'rejected' | 'expired') {
      outcome = kind
      clearTimeout(outcomeTimer)
      outcomeTimer = setTimeout(() => (outcome = ''), 2800)
    }

    function wait(p: Packet) {
      p.stage = 'wait'
      p.t = 0
      queue.push(p)
      if (queue.length === 1) promote()
    }

    decide = (approve: boolean) => {
      const p = queue.shift()
      if (!p) return
      const hadFocus = card.contains(document.activeElement)
      if (still) {
        packets = packets.filter((x) => x !== p)
        seals = approve ? [0.5] : []
      } else if (approve) {
        p.stage = 'through'
        p.t = p.lane.wait
        p.exit = icp
      } else {
        p.stage = 'back'
        p.t = 1 + p.lane.wait
      }
      if (approve) stats.approved++
      else stats.rejected++
      flash(approve ? 'approved' : 'rejected')
      // Without motion, the next request is already waiting.
      if (still && !queue.length)
        wait(spawn((p.lane.index + 1) % SOURCES.length, true))
      if (queue.length) promote()
      else showHead()
      if (hadFocus) card.focus()
      if (still) draw()
    }

    send = (src: number) => {
      pinged = src
      setTimeout(() => pinged === src && (pinged = -1), 600)
      const busy = pendingFormal()
      const formal = busy.length < 3 && !busy.some((p) => p.lane.index === src)
      const p = spawn(src, formal)
      if (!still) {
        if (formal) nextFormal = Math.max(nextFormal, 5)
        return
      }
      // Without motion a request arrives where it waits; other work is only
      // counted.
      if (formal) wait(p)
      else packets.pop()
      draw()
    }

    function step(dt: number) {
      clock += dt
      nextSync -= dt
      nextFormal -= dt
      if (nextSync <= 0) {
        const pick = Math.random()
        spawn(pick < 0.32 ? 0 : pick < 0.58 ? 1 : pick < 0.82 ? 2 : 3, false)
        nextSync = 0.55 + Math.random() * 0.8
      }
      if (nextFormal <= 0) {
        const busy = pendingFormal()
        if (busy.length < 2) {
          const free = [2, 3, 1, 0].filter(
            (s) => s !== lastFormal && !busy.some((p) => p.lane.index === s)
          )
          // The first request comes from an agent, the most telling case.
          const src =
            seq < 3 ? 2 : (free[(Math.random() * free.length) | 0] ?? 2)
          lastFormal = src
          spawn(src, true)
        }
        nextFormal = 6.5 + Math.random() * 3.5
      }
      for (const p of packets) {
        switch (p.stage) {
          case 'in':
            p.t += dt / (p.formal ? 2.6 : 2.2)
            if (p.t >= 1) {
              p.t = 0
              p.stage = p.formal ? 'approach' : 'through'
            }
            break
          case 'approach':
            p.t += dt / 0.4
            if (p.t >= 1) wait(p)
            break
          case 'wait':
            p.t += dt
            break
          case 'through':
            p.t += dt / (p.formal ? 0.7 : 0.8)
            if (p.t >= 1) {
              p.t = 0
              p.stage = 'out'
            }
            break
          case 'out':
            p.t += dt / (p.formal ? 1.5 : 2.3)
            for (let i = 0; i < p.cipher.length; i++)
              if (Math.random() < 0.1) p.cipher[i] = digit()
            if (p.t >= 1) {
              if (p.formal) seals.push(0)
              p.stage = 'done'
            }
            break
          case 'back':
            p.t -= dt / 1.3
            p.alpha = Math.max(0, Math.min(1, (p.t - 0.45) / 0.55))
            if (p.t <= 0.45) p.stage = 'done'
            break
          case 'fade':
            p.alpha -= dt / 0.8
            if (p.alpha <= 0) p.stage = 'done'
            break
        }
      }
      const first = queue[0]
      if (first && clock >= deadline) {
        queue.shift()
        first.at = position(first)
        first.stage = 'fade'
        stats.expired++
        flash('expired')
        if (queue.length) promote()
        else showHead()
      } else if (first && first.id !== shownId) promote()
      else if (first && Math.floor(clock * 4) !== Math.floor((clock - dt) * 4))
        showHead()
      packets = packets.filter((p) => p.stage !== 'done')
      seals = seals.map((s) => s + dt / 1.2).filter((s) => s < 1)
      draw()
    }

    function trace(points: Pt[], close = false) {
      ctx.beginPath()
      points.forEach((p, i) =>
        i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)
      )
      if (close) ctx.closePath()
    }

    function bezier([a, b, c, d]: Curve) {
      ctx.beginPath()
      ctx.moveTo(a.x, a.y)
      ctx.bezierCurveTo(b.x, b.y, c.x, c.y, d.x, d.y)
    }

    function diamond(p: Pt, size: number) {
      trace(
        [
          { x: p.x, y: p.y - size },
          { x: p.x + size, y: p.y },
          { x: p.x, y: p.y + size },
          { x: p.x - size, y: p.y }
        ],
        true
      )
    }

    function draw() {
      ctx.clearRect(0, 0, width, height)

      // A field of graph-paper dots that leans away from the pointer.
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

      // Paths in, and the two ways out.
      ctx.lineCap = 'round'
      for (const lane of lanes) {
        const hot = hover === lane.index
        bezier(lane.src)
        ctx.lineWidth = hot ? 1.6 : 1
        ctx.strokeStyle = hot ? `rgba(${FOREST}, 0.75)` : `rgba(${INK}, 0.2)`
        ctx.stroke()
      }
      bezier(cloud.curve)
      ctx.lineWidth = 1
      ctx.strokeStyle = `rgba(${JADE}, 0.42)`
      ctx.stroke()
      ctx.setLineDash([3, 5])
      bezier(icp.curve)
      ctx.strokeStyle = `rgba(${INK}, 0.3)`
      ctx.stroke()
      ctx.setLineDash([])

      // The gate: your computer, with the approval queue inside.
      ctx.save()
      ctx.shadowColor = 'rgba(23, 61, 41, 0.12)'
      ctx.shadowBlur = 28
      ctx.shadowOffsetY = 10
      trace(gate, true)
      ctx.fillStyle = '#ffffff'
      ctx.fill()
      ctx.restore()
      trace(gate, true)
      ctx.lineWidth = 1.5
      ctx.strokeStyle = `rgba(${INK}, 0.9)`
      ctx.stroke()
      trace(core, true)
      ctx.lineWidth = 1
      ctx.strokeStyle = `rgba(${INK}, 0.14)`
      ctx.stroke()
      if (queue.length) {
        ctx.setLineDash([4, 4])
        trace(coreEdge)
        ctx.lineWidth = 1.5
        ctx.strokeStyle = `rgba(${AMBER}, 0.7)`
        ctx.stroke()
        ctx.setLineDash([])
      }

      for (const p of anchors.src) {
        ctx.beginPath()
        ctx.arc(p.x, p.y, 3, 0, Math.PI * 2)
        ctx.fillStyle = '#f7f8f4'
        ctx.fill()
        ctx.lineWidth = 1.2
        ctx.strokeStyle = `rgba(${INK}, 0.7)`
        ctx.stroke()
      }
      anchors.dst.forEach((p, j) => {
        ctx.beginPath()
        ctx.arc(p.x, p.y, 4.5, 0, Math.PI * 2)
        ctx.fillStyle = j ? `rgba(${FOREST}, 1)` : '#f7f8f4'
        ctx.fill()
        ctx.lineWidth = 1.2
        ctx.strokeStyle = j ? `rgba(${FOREST}, 1)` : `rgba(${JADE}, 0.9)`
        ctx.stroke()
      })
      const sealAt = anchors.dst[1]
      if (sealAt)
        for (const s of seals) {
          ctx.beginPath()
          ctx.arc(sealAt.x, sealAt.y, 5 + s * 26, 0, Math.PI * 2)
          ctx.lineWidth = 1.5
          ctx.strokeStyle = `rgba(${FOREST}, ${0.55 * (1 - s)})`
          ctx.stroke()
        }

      // A leader from the request in front to the card below the figure.
      const first = queue[0]
      if (first) {
        const p = position(first)
        ctx.setLineDash([2, 4])
        ctx.beginPath()
        ctx.moveTo(p.x, p.y + 10)
        ctx.bezierCurveTo(
          p.x,
          p.y + 60,
          width / 2,
          height - 50,
          width / 2,
          height
        )
        ctx.lineWidth = 1
        ctx.strokeStyle = `rgba(${AMBER}, 0.55)`
        ctx.stroke()
        ctx.setLineDash([])
      }

      ctx.font =
        '500 10px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace'
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      for (const p of packets) {
        const q = position(p)
        if (!p.formal) {
          if (p.stage === 'out') {
            // Leaving the device, a request is only ciphertext.
            const step = 9 / p.exit.length
            p.cipher.forEach((char, k) => {
              const tt = p.t - k * step
              if (tt < 0) return
              const c = at(p.exit.curve, tt)
              ctx.fillStyle = `rgba(${JADE}, ${0.9 * (1 - k / p.cipher.length)})`
              ctx.fillText(char, c.x, c.y)
            })
            continue
          }
          if (p.stage === 'in')
            for (let k = 1; k < 5; k++) {
              const tt = p.t - k * 0.014
              if (tt < 0) break
              const c = at(p.lane.src, tt)
              ctx.beginPath()
              ctx.arc(c.x, c.y, 2.4 - k * 0.4, 0, Math.PI * 2)
              ctx.fillStyle = `rgba(${FOREST}, ${0.35 - k * 0.07})`
              ctx.fill()
            }
          ctx.beginPath()
          ctx.arc(q.x, q.y, 2.6, 0, Math.PI * 2)
          ctx.fillStyle = `rgba(${FOREST}, ${p.stage === 'through' ? 0.35 : 0.9})`
          ctx.fill()
          continue
        }
        const front = p === first
        const size = front ? 7.5 : 6.5
        diamond(q, size)
        if (p.stage === 'wait') {
          ctx.fillStyle = `rgba(${AMBER}, ${front ? 1 : 0.55})`
          ctx.fill()
          if (!still) {
            const phase = (clock * 0.7 + p.id * 0.37) % 1
            ctx.beginPath()
            ctx.arc(q.x, q.y, 10 + phase * 9, 0, Math.PI * 2)
            ctx.lineWidth = 1
            ctx.strokeStyle = `rgba(${AMBER}, ${0.35 * (1 - phase)})`
            ctx.stroke()
          }
        } else if (p.stage === 'through' || p.stage === 'out') {
          ctx.fillStyle = `rgba(${FOREST}, 1)`
          ctx.fill()
        } else if (p.stage === 'back' || p.stage === 'fade') {
          ctx.lineWidth = 1.4
          ctx.strokeStyle = `rgba(78, 98, 87, ${p.alpha})`
          ctx.stroke()
        } else {
          ctx.fillStyle = '#f7f8f4'
          ctx.fill()
          ctx.lineWidth = 1.6
          ctx.strokeStyle = `rgba(${AMBER}, 0.95)`
          ctx.stroke()
        }
      }
    }

    function seedStill() {
      const place = (src: number, stage: Stage, t: number) => {
        const p = spawn(src, false)
        p.stage = stage
        p.t = t
      }
      place(0, 'in', 0.42)
      place(3, 'in', 0.7)
      place(1, 'out', 0.42)
      place(0, 'out', 0.8)
      wait(spawn(2, true))
    }

    layout()
    const loop = frameLoop(step, () => visible && !still)
    const resize = new ResizeObserver(() => {
      layout()
      draw()
    })
    resize.observe(stage)
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
      stage.removeEventListener('pointermove', onMove)
      stage.removeEventListener('pointerleave', onLeave)
    }
  })
</script>

<figure class="fig gate-fig">
  <div class="gate-frame">
    <div class="gate-stage" class:vertical bind:this={stage} dir="ltr">
      <canvas bind:this={canvas} aria-hidden="true"></canvas>
      {#each SOURCES as source, i}
        <button
          class="gate-source"
          class:hot={hover === i}
          class:ping={pinged === i}
          style:left="{anchors.src[i]?.x ?? 0}px"
          style:top="{anchors.src[i]?.y ?? 0}px"
          aria-label={$t('Send a request from {source}', {
            source: names[i] ?? ''
          })}
          onpointerenter={() => (hover = i)}
          onpointerleave={() => (hover = -1)}
          onfocus={() => (hover = i)}
          onblur={() => (hover = -1)}
          onclick={() => send(i)}
        >
          <span class="gs-icon"><Glyph name={source.glyph} size={18} /></span>
          <span class="gs-text" dir="auto"
            ><strong>{vertical ? shortNames[i] : names[i]}</strong><code
              >{source.tag}</code
            ></span
          >
        </button>
      {/each}
      <div
        class="gate-core"
        style:left="{anchors.core.x}px"
        style:top="{anchors.core.y}px"
        style:width="{anchors.r * (vertical ? 1.15 : 0.84)}px"
      >
        <code>{DAEMON}</code>
        <span dir="auto">{$t('Keys stay here')}</span>
      </div>
      {#each anchors.dst as point, j}
        <div
          class="gate-dest"
          class:icp={j === 1}
          style:left="{point.x}px"
          style:top="{point.y}px"
        >
          <span class="gd-icon"
            ><Glyph name={j ? 'badge' : 'cloud'} size={16} /></span
          >
          <span class="gs-text" dir="auto"
            ><strong>{j ? ICP : $t('dMsg cloud')}</strong><small
              >{j
                ? $t('Identity and signatures')
                : $t('Ciphertext only')}</small
            ></span
          >
        </div>
      {/each}
    </div>

    <div class="gate-dock">
      <ul class="gate-legend">
        <li><i class="lg-dot"></i>{$t('Everyday work, encrypted')}</li>
        <li><i class="lg-diamond"></i>{$t('Needs your approval')}</li>
      </ul>
      <div
        class="gate-card"
        class:waiting={head}
        bind:this={card}
        tabindex="-1"
        role="group"
        aria-label={$t('Approval')}
      >
        {#if head && waiting}
          <div class="gc-head">
            <span class="gc-from"
              ><Glyph name={waiting.glyph} size={16} />{names[head.src]}</span
            >
            <span class="gc-timer">0:{pad(head.left)}</span>
          </div>
          <p class="gc-ask">
            <strong>{asks[head.src]}</strong>
            <code>{waiting.object}</code>
          </p>
          <div class="gc-actions">
            <button class="button secondary" onclick={() => decide(false)}
              >{$t('Reject')}</button
            >
            <button class="button primary" onclick={() => decide(true)}
              >{$t('Approve')}</button
            >
          </div>
          <i class="gc-expiry" style:transform="scaleX({head.share})"></i>
        {:else}
          <p class="gc-idle">
            {$t('Nothing is waiting. Tap a source to send a request.')}
          </p>
        {/if}
      </div>
      <dl class="gate-stats">
        <div><dt>{$t('Requests')}</dt><dd>{stats.seen}</dd></div>
        <div><dt>{$t('Approved by you')}</dt><dd>{stats.approved}</dd></div>
        <div><dt>{$t('Rejected')}</dt><dd>{stats.rejected}</dd></div>
        <div><dt>{$t('Expired')}</dt><dd>{stats.expired}</dd></div>
        <div class="zero"><dt>{$t('Signed without you')}</dt><dd>0</dd></div>
      </dl>
    </div>
  </div>
  <p class="gate-outcome" class:shown={outcome} aria-live="polite">
    {outcomeText}
  </p>
  <p class="sr-only">
    {$t(
      'Requests from the desktop app, the terminal, AI agents and websites reach the dMsg daemon on your computer. Everyday work leaves only as ciphertext for the cloud; a signature reaches ICP only after you approve it.'
    )}
  </p>
  <figcaption>
    <span class="fig-no">{$t('Fig. {n}', { n: 1 })}</span>
    <span>{$t('Everything asks. Only you approve.')}</span>
    <span class="fig-note"
      ><i class="live"></i>{$t('Simulation · nothing is signed')}</span
    >
  </figcaption>
</figure>
