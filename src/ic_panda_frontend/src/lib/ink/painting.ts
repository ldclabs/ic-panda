/*
  A live ink painting of a panda eating bamboo, painted by an invisible hand
  on a fixed canvas behind the page. The ink engine (splat → bleed → paper →
  display) is adapted from 墨韵 Moyun, MIT License, Copyright (c) 2026 Axton
  Liu; the panda is a signed distance pass drawn over it every frame.
*/
import {
  clamp01,
  createRig,
  gazeAt,
  hitPanda,
  pokeRig,
  resetRig,
  rigTick,
  wakeRig
} from './rig'
import {
  buildScene,
  computeLayout,
  keepOut,
  TAPER,
  type Item,
  type Layout,
  type Pt,
  type Stroke
} from './scene'
import { FS, HEAD, VS, type ProgramName } from './shaders'

export interface InkHost {
  /** The hero sheet and the header's bottom edge, in viewport pixels at scroll 0. */
  measure(): { sheet: { top: number; right: number }; barBottom: number }
  /** Pointer events on these targets never reach the panda. */
  isUI(target: EventTarget | null): boolean
  onSeed(seed: number): void
  onCue(cue: 'colophon' | 'seal' | 'clear'): void
  onPoke(): void
}

export interface InkPainting {
  repaint(): void
  /** Drop to a lower frame rate while the painting is mostly covered. */
  setQuiet(quiet: boolean): void
  destroy(): void
}

/* Xuan paper, pine-soot ink, cinnabar and mineral green. */
const THEME = {
  paper: rgb(0xece9e1),
  paper2: rgb(0xdcd6c8),
  ink: rgb(0x17181b),
  cin: rgb(0xb23b2b),
  jade: rgb(0x1e9e7e)
}

function rgb(n: number): [number, number, number] {
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255]
}

interface Fbo {
  tex: WebGLTexture
  fb: WebGLFramebuffer
  w: number
  h: number
  bind(unit: number): number
}

interface DoubleFbo {
  w: number
  h: number
  read: Fbo
  write: Fbo
  swap(): void
}

type Uniforms = (name: string) => WebGLUniformLocation | null

interface Dab {
  x: number
  y: number
  r2: number
  hard: number
  a: number
  c: number
  w: number
  g: number
  dry: number
  dx: number
  dy: number
  seed: number
}

interface ActiveStroke extends Stroke {
  cum: number[]
  total: number
  s: number
  k: number
  prev: Pt | null
}

interface ActivePart {
  kind: 'part'
  part: number
  dur: number
  path: Pt[]
  after: number
  t: number
}

export function createInkPainting(
  canvas: HTMLCanvasElement,
  ghostEl: HTMLElement,
  host: InkHost,
  reduced: boolean,
  delay = 0
): InkPainting | null {
  const dev = import.meta.env.DEV
  const params = new URLSearchParams(location.search)
  // ?still previews reduced motion; ?debug keeps the drawing buffer so the
  // canvas can be copied for inspection. Both only in development.
  if (dev && params.has('still')) reduced = true
  const coarse = matchMedia('(pointer: coarse)').matches

  const gl = canvas.getContext('webgl2', {
    alpha: false,
    depth: false,
    stencil: false,
    antialias: false,
    premultipliedAlpha: false,
    preserveDrawingBuffer: dev && params.has('debug')
  })
  if (!gl || !gl.getExtension('EXT_color_buffer_float')) return null
  gl.getExtension('OES_texture_float_linear')

  /* ───────── programs ───────── */
  function compile(type: number, src: string) {
    const s = gl!.createShader(type)!
    gl!.shaderSource(s, src)
    gl!.compileShader(s)
    if (!gl!.getShaderParameter(s, gl!.COMPILE_STATUS))
      throw new Error(gl!.getShaderInfoLog(s) ?? 'shader compile failed')
    return s
  }
  let P: Record<ProgramName, { use(): Uniforms }>
  try {
    const vs = compile(gl.VERTEX_SHADER, VS)
    const mk = (fs: string) => {
      const p = gl.createProgram()
      gl.attachShader(p, vs)
      gl.attachShader(p, compile(gl.FRAGMENT_SHADER, HEAD + fs))
      gl.bindAttribLocation(p, 0, 'aPos')
      gl.linkProgram(p)
      if (!gl.getProgramParameter(p, gl.LINK_STATUS))
        throw new Error(gl.getProgramInfoLog(p) ?? 'program link failed')
      const u = new Map<string, WebGLUniformLocation | null>()
      const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS) as number
      for (let i = 0; i < n; i++) {
        const name = gl.getActiveUniform(p, i)!.name.replace(/\[0\]$/, '')
        u.set(name, gl.getUniformLocation(p, name))
      }
      const get: Uniforms = (name) => u.get(name) ?? null
      return {
        use() {
          gl.useProgram(p)
          return get
        }
      }
    }
    P = Object.fromEntries(
      Object.entries(FS).map(([k, src]) => [k, mk(src)])
    ) as typeof P
  } catch (err) {
    console.error('Ink painting unavailable:', err)
    return null
  }

  const vao = gl.createVertexArray()
  gl.bindVertexArray(vao)
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer())
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([-1, -1, -1, 1, 1, 1, 1, -1]),
    gl.STATIC_DRAW
  )
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, gl.createBuffer())
  gl.bufferData(
    gl.ELEMENT_ARRAY_BUFFER,
    new Uint16Array([0, 1, 2, 0, 2, 3]),
    gl.STATIC_DRAW
  )
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0)
  gl.enableVertexAttribArray(0)

  function blit(t: Fbo | null) {
    if (t) {
      gl!.bindFramebuffer(gl!.FRAMEBUFFER, t.fb)
      gl!.viewport(0, 0, t.w, t.h)
    } else {
      gl!.bindFramebuffer(gl!.FRAMEBUFFER, null)
      gl!.viewport(0, 0, gl!.drawingBufferWidth, gl!.drawingBufferHeight)
    }
    gl!.drawElements(gl!.TRIANGLES, 6, gl!.UNSIGNED_SHORT, 0)
  }
  function fbo(
    w: number,
    h: number,
    internal: number,
    format: number,
    type: number,
    filter: number
  ): Fbo {
    const g = gl!
    g.activeTexture(g.TEXTURE0)
    const tex = g.createTexture()
    g.bindTexture(g.TEXTURE_2D, tex)
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MIN_FILTER, filter)
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_MAG_FILTER, filter)
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_S, g.CLAMP_TO_EDGE)
    g.texParameteri(g.TEXTURE_2D, g.TEXTURE_WRAP_T, g.CLAMP_TO_EDGE)
    g.texImage2D(g.TEXTURE_2D, 0, internal, w, h, 0, format, type, null)
    const fb = g.createFramebuffer()
    g.bindFramebuffer(g.FRAMEBUFFER, fb)
    g.framebufferTexture2D(
      g.FRAMEBUFFER,
      g.COLOR_ATTACHMENT0,
      g.TEXTURE_2D,
      tex,
      0
    )
    g.viewport(0, 0, w, h)
    g.clearColor(0, 0, 0, 0)
    g.clear(g.COLOR_BUFFER_BIT)
    return {
      tex,
      fb,
      w,
      h,
      bind(unit) {
        g.activeTexture(g.TEXTURE0 + unit)
        g.bindTexture(g.TEXTURE_2D, tex)
        return unit
      }
    }
  }
  function dbl(w: number, h: number): DoubleFbo {
    const mk = () =>
      fbo(w, h, gl!.RGBA16F, gl!.RGBA, gl!.HALF_FLOAT, gl!.LINEAR)
    let r = mk()
    let wr = mk()
    return {
      w,
      h,
      get read() {
        return r
      },
      get write() {
        return wr
      },
      swap() {
        const t = r
        r = wr
        wr = t
      }
    }
  }
  function free(f: Fbo) {
    gl!.deleteTexture(f.tex)
    gl!.deleteFramebuffer(f.fb)
  }

  /* ───────── surface ───────── */
  const DYE = coarse ? 640 : 900
  let dye: DoubleFbo | null = null
  let paperF: Fbo | null = null
  let dpr = 1
  let W = 1
  let H = 1
  const paperSeed = Math.random() * 50

  function gridRes(n: number): [number, number] {
    const w = gl!.drawingBufferWidth
    const h = gl!.drawingBufferHeight
    let a = w / h
    if (a < 1) a = 1 / a
    const mn = Math.round(n)
    const mx = Math.round(n * a)
    return w > h ? [mx, mn] : [mn, mx]
  }
  function bakePaper() {
    const u = P.paper.use()
    gl!.uniform2f(u('res'), canvas.width / dpr, canvas.height / dpr)
    gl!.uniform3fv(u('paper'), THEME.paper)
    gl!.uniform3fv(u('paper2'), THEME.paper2)
    gl!.uniform1f(u('seed'), paperSeed)
    blit(paperF)
  }
  function resize() {
    W = innerWidth
    H = innerHeight
    dpr = Math.min(devicePixelRatio || 1, coarse ? 1.5 : 2)
    const w = Math.max(2, Math.round(W * dpr))
    const h = Math.max(2, Math.round(H * dpr))
    if (w === canvas.width && h === canvas.height && dye) return
    canvas.width = w
    canvas.height = h
    const [dw, dh] = gridRes(DYE)
    const nd = dbl(dw, dh)
    if (dye) {
      const u = P.copy.use()
      gl!.uniform1i(u('uTexture'), dye.read.bind(0))
      blit(nd.read)
      free(dye.read)
      free(dye.write)
    }
    dye = nd
    if (paperF) free(paperF)
    paperF = fbo(w, h, gl!.RGBA8, gl!.RGBA, gl!.UNSIGNED_BYTE, gl!.LINEAR)
    bakePaper()
  }
  const aspect = () => canvas.width / canvas.height

  /* ───────── splats ───────── */
  const MAXS = 32
  const posA = new Float32Array(MAXS * 4)
  const valA = new Float32Array(MAXS * 4)
  const dirA = new Float32Array(MAXS * 4)
  const dyeQ: Dab[] = []

  function flush() {
    const d = dye!
    let i = 0
    while (i < dyeQ.length) {
      const n = Math.min(MAXS, dyeQ.length - i)
      posA.fill(0)
      valA.fill(0)
      dirA.fill(0)
      for (let k = 0; k < n; k++) {
        const s = dyeQ[i + k]!
        const o = k * 4
        posA[o] = s.x
        posA[o + 1] = s.y
        posA[o + 2] = Math.max(s.r2, 1e-7)
        posA[o + 3] = s.hard
        valA[o] = s.a
        valA[o + 1] = s.c
        valA[o + 2] = s.w
        valA[o + 3] = s.dry
        dirA[o] = s.dx
        dirA[o + 1] = s.dy
        dirA[o + 2] = s.seed
        dirA[o + 3] = s.g
      }
      const u = P.splat.use()
      gl!.uniform1i(u('uTarget'), d.read.bind(0))
      gl!.uniform1f(u('aspect'), aspect())
      gl!.uniform1i(u('count'), n)
      gl!.uniform2f(u('res'), d.w, d.h)
      gl!.uniform1f(u('lim'), 4)
      gl!.uniform4fv(u('uPos'), posA)
      gl!.uniform4fv(u('uVal'), valA)
      gl!.uniform4fv(u('uDir'), dirA)
      blit(d.write)
      d.swap()
      i += n
    }
    dyeQ.length = 0
  }

  /* One brush segment: gaussian stamps at even spacing, ink normalised by spacing. */
  function brushSeg(
    x0: number,
    y0: number,
    x1: number,
    y1: number,
    st: {
      r0: number
      r1: number
      ink: number
      cin: number
      green: number
      water: number
      dry: number
      seed: number
      hard: number
    }
  ) {
    const A = aspect()
    const dxh = (x1 - x0) * A
    const dyh = y1 - y0
    const len = Math.hypot(dxh, dyh)
    const spacing = Math.max(((st.r0 + st.r1) / 2) * 0.45, 0.0012)
    const n = Math.max(1, Math.ceil(len / spacing))
    const step = len / n
    const dx = len > 0 ? dxh / len : 1
    const dy = len > 0 ? dyh / len : 0
    for (let i = 1; i <= n; i++) {
      const t = i / n
      const x = x0 + (x1 - x0) * t
      const y = y0 + (y1 - y0) * t
      if (L && keepOut(L, x, y)) continue
      const r = st.r0 + (st.r1 - st.r0) * t
      const amt = Math.min(1, Math.max(step, r * 0.3) / (r * 1.77))
      dyeQ.push({
        x,
        y,
        r2: r * r,
        hard: st.hard,
        a: st.ink * amt,
        c: st.cin * amt,
        w: st.water * amt,
        g: st.green * amt,
        dry: st.dry,
        dx,
        dy,
        seed: st.seed
      })
    }
  }

  /* ───────── bleed ───────── */
  let awakeUntil = 0
  let simAcc = 0
  function bleed() {
    const d = dye!
    const u = P.bleed.use()
    gl!.uniform2f(u('texel'), 1 / d.w, 1 / d.h)
    gl!.uniform2f(u('res'), d.w, d.h)
    gl!.uniform1f(u('rate'), 0.75)
    gl!.uniform1f(u('evap'), 0.99)
    for (let i = 0; i < 3; i++) {
      gl!.uniform1i(u('uDye'), d.read.bind(0))
      blit(d.write)
      d.swap()
    }
  }
  function scaleDye(v: number) {
    const d = dye!
    const u = P.clear.use()
    gl!.uniform1f(u('value'), v)
    gl!.uniform1i(u('uTexture'), d.read.bind(0))
    blit(d.write)
    d.swap()
  }

  /* ───────── layout ───────── */
  let L: Layout | null = null
  function layout() {
    const m = host.measure()
    L = computeLayout(W, H, m.sheet, m.barBottom)
  }

  /* ───────── the invisible hand ───────── */
  const ghost = {
    q: [] as Item[],
    cur: null as ActiveStroke | ActivePart | null,
    wait: 0,
    on: false,
    speed: 2,
    px: 0,
    py: 0
  }
  function moveGhost(x: number, y: number, r: number) {
    const s = Math.max(0.5, (r * H * 2.2) / 14)
    ghost.px = x * W
    ghost.py = (1 - y) * H
    ghostEl.style.transform = `translate(${ghost.px}px,${ghost.py}px) scale(${s})`
  }
  function prep(it: Stroke): ActiveStroke {
    const A = aspect()
    const cum = [0]
    for (let i = 1; i < it.pts.length; i++) {
      const a = it.pts[i - 1]!
      const b = it.pts[i]!
      cum.push(cum[i - 1]! + Math.hypot((b[0] - a[0]) * A, b[1] - a[1]))
    }
    return { ...it, cum, total: cum[cum.length - 1]!, s: 0, k: 0, prev: null }
  }
  function posAt(c: ActiveStroke, s: number): Pt {
    while (c.k < c.cum.length - 2 && c.cum[c.k + 1]! < s) c.k++
    const a = c.pts[c.k]!
    const b = c.pts[c.k + 1] ?? a
    const len = (c.cum[c.k + 1] ?? 0) - c.cum[c.k]!
    const t = len > 0 ? Math.min(1, (s - c.cum[c.k]!) / len) : 0
    return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
  }
  function emitAlong(c: ActiveStroke, s0: number, s1: number) {
    let s = s0
    let p = c.prev ?? posAt(c, s0)
    const tp = TAPER[c.tp]
    while (s < s1 - 1e-9) {
      const s2 = Math.min(s1, s + 0.008)
      const q = posAt(c, s2)
      const t0 = s / c.total
      const t1 = s2 / c.total
      brushSeg(p[0], p[1], q[0], q[1], {
        r0: c.r * tp(t0),
        r1: c.r * tp(t1),
        ink: c.ink * (1 - c.fade * t1),
        cin: c.cin,
        green: c.green,
        water: c.water,
        dry: Math.min(0.95, c.dry + c.dryGrow * t1),
        seed: c.seed,
        hard: c.hard
      })
      p = q
      s = s2
    }
    c.prev = p
    moveGhost(p[0], p[1], c.r)
  }
  function pathAt(path: Pt[], t: number): Pt {
    const n = path.length - 1
    if (n <= 0) return path[0]!
    const f = Math.min(n - 1e-6, t * n)
    const i = Math.floor(f)
    const k = f - i
    const a = path[i]!
    const b = path[i + 1]!
    return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
  }
  function cue(c: 'wake' | 'colophon' | 'seal') {
    if (c === 'wake') wakeRig(rig)
    else host.onCue(c)
  }
  function ghostTick(dt: number) {
    if (!ghost.on) return
    let budget = dt * ghost.speed
    let guard = 0
    while (budget > 0 && guard++ < 4000) {
      if (!ghost.cur) {
        if (ghost.wait > 0) {
          const w = Math.min(ghost.wait, budget)
          ghost.wait -= w
          budget -= w
          continue
        }
        const it = ghost.q.shift()
        if (!it) {
          ghost.on = false
          ghostEl.classList.remove('on')
          return
        }
        if (it.kind === 'cue') {
          cue(it.cue)
          ghost.wait = it.after
          continue
        }
        if (it.kind === 'dot') {
          dyeQ.push({
            x: it.x,
            y: it.y,
            r2: it.r * it.r,
            hard: it.hard,
            a: it.ink,
            c: it.cin,
            w: it.water,
            g: it.green,
            dry: 0,
            dx: 1,
            dy: 0,
            seed: 0
          })
          moveGhost(it.x, it.y, it.r)
          ghost.wait = it.after
          continue
        }
        if (it.kind === 'part') {
          ghost.cur = { ...it, t: 0 }
          continue
        }
        const c = prep(it)
        if (c.total <= 0) continue
        ghost.cur = c
      }
      const c = ghost.cur
      if (c.kind === 'part') {
        const use = Math.min(c.dur - c.t, budget)
        c.t += use
        budget -= use
        rig.rev[c.part] = c.t / c.dur
        const q = pathAt(c.path, c.t / c.dur)
        moveGhost(q[0], q[1], 0.006)
        if (c.t >= c.dur - 1e-6) {
          rig.rev[c.part] = 1
          ghost.cur = null
          ghost.wait = c.after
        }
        continue
      }
      const use = Math.min((c.total - c.s) / c.speed, budget)
      const s1 = Math.min(c.total, c.s + use * c.speed)
      emitAlong(c, c.s, s1)
      c.s = s1
      budget -= use
      if (c.s >= c.total - 1e-6) {
        ghost.cur = null
        ghost.wait = c.after
      }
    }
  }
  function stopGhost() {
    ghost.on = false
    ghost.q = []
    ghost.cur = null
    ghostEl.classList.remove('on')
  }

  /* ───────── the panda ───────── */
  const rig = createRig()
  const pointer = { x: 0, y: 0, t: -1e9, in: false } // t is on the rig's clock

  /* ───────── painting lifecycle ───────── */
  let fading = 0
  function start() {
    layout()
    const seed = (Math.random() * 0xffffff) | 0
    host.onSeed(seed)
    resetRig(rig)
    rig.alpha = 1
    ghost.q = buildScene(seed, L!)
    ghost.cur = null
    ghost.wait = 0.2
    ghost.on = true
    if (reduced) fastForward()
    else ghostEl.classList.add('on')
  }
  /* Reduced motion: paint the whole thing in one go and let it settle unseen. */
  function fastForward() {
    const sp = ghost.speed
    ghost.speed = 12
    for (let i = 0; i < 110; i++) {
      ghostTick(1 / 30)
      flush()
      bleed()
    }
    ghost.speed = sp
    awakeUntil = performance.now() + 4000
  }
  function repaint() {
    stopGhost()
    host.onCue('clear')
    if (reduced) {
      scaleDye(0)
      start()
      return
    }
    fading = 0.6
  }

  /* ───────── render ───────── */
  function render() {
    const d = dye!
    let u = P.disp.use()
    gl!.uniform1i(u('uDye'), d.read.bind(0))
    gl!.uniform1i(u('uPaper'), paperF!.bind(1))
    gl!.uniform2f(u('dtex'), 1 / d.w, 1 / d.h)
    gl!.uniform3fv(u('ink'), THEME.ink)
    gl!.uniform3fv(u('cin'), THEME.cin)
    gl!.uniform3fv(u('jade'), THEME.jade)
    blit(null)
    if (!L?.ph || rig.alpha <= 0) return
    const s = L.ph * dpr
    const ox = L.cx * dpr
    const oy = (H - L.base) * dpr
    const x0 = Math.max(0, Math.floor(ox - s * 0.95))
    const x1 = Math.min(canvas.width, Math.ceil(ox + s * 0.65))
    const y0 = Math.max(0, Math.floor(oy - s * 0.15))
    const y1 = Math.min(canvas.height, Math.ceil(oy + s * 1.18))
    if (x1 <= x0 || y1 <= y0) return
    const g = gl!
    g.enable(g.SCISSOR_TEST)
    g.scissor(x0, y0, x1 - x0, y1 - y0)
    g.enable(g.BLEND)
    g.blendFunc(g.ONE, g.ONE_MINUS_SRC_ALPHA)
    u = P.panda.use()
    g.uniform1i(u('uPaper'), paperF!.bind(0))
    g.uniform2f(u('res'), canvas.width, canvas.height)
    g.uniform4f(u('uP'), ox, oy, s, rig.alpha)
    g.uniform3fv(u('ink'), THEME.ink)
    g.uniform3fv(u('jade'), THEME.jade)
    g.uniform4f(u('uBody'), rig.breath, 0, 0, 0)
    g.uniform4f(
      u('uHead'),
      rig.turn.x,
      rig.nod.x,
      rig.tilt.x,
      clamp01(rig.jaw.x)
    )
    g.uniform4f(u('uHeadO'), rig.hx.x, rig.hy.x, 0, 0)
    g.uniform4f(u('uEar'), rig.earL.x * 0.35, rig.earR.x * 0.35, 0, 0)
    g.uniform4f(u('uEye'), rig.gx.x, rig.gy.x, rig.open, rig.wide)
    g.uniform4f(u('uArm'), rig.pawX.x, rig.pawY.x, 0, rig.rustle)
    g.uniform1fv(u('uRev'), rig.rev)
    blit(null)
    g.disable(g.BLEND)
    g.disable(g.SCISSOR_TEST)
  }

  /* ───────── input ───────── */
  const root = document.documentElement
  let hovering = false
  const onMove = (e: PointerEvent) => {
    pointer.x = e.clientX
    pointer.y = e.clientY
    pointer.t = rig.t
    pointer.in = true
    const over =
      !host.isUI(e.target) && rig.alive && hitPanda(L, e.clientX, e.clientY)
    if (over !== hovering) {
      hovering = over
      root.style.cursor = over ? 'pointer' : ''
    }
  }
  const onLeave = () => {
    pointer.in = false
  }
  const onDown = (e: PointerEvent) => {
    if (host.isUI(e.target) || !hitPanda(L, e.clientX, e.clientY)) return
    if (pokeRig(rig)) host.onPoke()
  }
  let resizeTimer = 0
  let lastW = innerWidth
  let lastH = innerHeight
  const onResize = () => {
    clearTimeout(resizeTimer)
    resizeTimer = window.setTimeout(() => {
      // Mobile browsers resize a little as their toolbars come and go; only
      // a real change of shape repaints.
      const big =
        Math.abs(innerWidth - lastW) > 1 ||
        Math.abs(innerHeight - lastH) / lastH > 0.2
      resize()
      if (!big) return
      lastW = innerWidth
      lastH = innerHeight
      stopGhost()
      host.onCue('clear')
      scaleDye(0)
      start()
      if (!reduced) fastForward()
    }, 250)
  }
  addEventListener('pointermove', onMove, { passive: true })
  root.addEventListener('pointerleave', onLeave)
  addEventListener('pointerdown', onDown)
  addEventListener('resize', onResize)

  /* ───────── loop ───────── */
  let last = performance.now()
  let raf = 0
  let quiet = false
  let skip = 0
  function advanceBy(dt: number, now: number, draw: boolean) {
    if (fading > 0) {
      fading -= dt
      scaleDye(0.86)
      rig.alpha = Math.max(0, fading / 0.6)
      awakeUntil = now + 500
      if (fading <= 0) {
        scaleDye(0)
        start()
      }
    }
    ghostTick(dt)
    if (dyeQ.length) {
      flush()
      awakeUntil = now + 5000
    }
    if (now < awakeUntil) {
      simAcc = Math.min(simAcc + dt, 2 / 60)
      while (simAcc >= 1 / 60) {
        bleed()
        simAcc -= 1 / 60
      }
    }
    const moving = pointer.in && rig.t - pointer.t < 2.5
    const target = !L
      ? null
      : moving
        ? gazeAt(L, pointer.x, pointer.y)
        : ghost.on
          ? gazeAt(L, ghost.px, ghost.py)
          : null
    rigTick(rig, dt, target, reduced)
    if (draw) render()
  }
  function frame(now: number) {
    raf = requestAnimationFrame(frame)
    const dt = Math.min(Math.max(1e-3, (now - last) / 1000), 1 / 20)
    last = now
    // While sheets cover the painting, every third frame is plenty.
    const draw = !quiet || ghost.on || fading > 0 || ++skip % 3 === 0
    advanceBy(dt, now, draw)
  }

  // The paper shows at once; the hand starts once the sheet has settled.
  resize()
  const startTimer = window.setTimeout(start, reduced ? 0 : delay)
  raf = requestAnimationFrame(frame)

  if (dev) {
    // Step the piece without rAF: a hidden tab never gets frames.
    ;(window as unknown as { __ink: unknown }).__ink = {
      ghost,
      rig,
      get L() {
        return L
      },
      advance(sec: number) {
        for (let t = 0; t < sec; t += 1 / 60) {
          last += 1000 / 60
          advanceBy(1 / 60, last, true)
        }
      }
    }
  }

  return {
    repaint,
    setQuiet(q) {
      quiet = q
    },
    destroy() {
      cancelAnimationFrame(raf)
      clearTimeout(startTimer)
      clearTimeout(resizeTimer)
      removeEventListener('pointermove', onMove)
      root.removeEventListener('pointerleave', onLeave)
      removeEventListener('pointerdown', onDown)
      removeEventListener('resize', onResize)
      if (hovering) root.style.cursor = ''
      gl.getExtension('WEBGL_lose_context')?.loseContext()
    }
  }
}
