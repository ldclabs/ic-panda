/*
  The composition: a few pale hills, then her, then the bamboo, the bank she
  sits on, a main peak, a sun and two birds, then the inscription and the
  seal. Everything is a queue of brush strokes, dots, panda parts and cues
  that an invisible hand works through in order.
*/

export type Pt = [number, number]

/** Where the painting leaves room for the hero sheet, in CSS pixels. */
export interface Layout {
  W: number
  H: number
  A: number
  wide: boolean
  /** Panda origin (ground under her seat) and height on screen. */
  cx: number
  base: number
  ph: number
  /** The same in canvas uv (y up) and her height as a uv fraction. */
  pu: number
  pv: number
  ux: number
  uy: number
  /** Right edge of the sheet, as a fraction of the width. */
  sheetR: number
  /** Bottom of the header. */
  barB: number
}

/**
 * Wide screens: the sheet on the left, her on the right. Narrow or portrait
 * ones: her between the header and the sheet, which starts lower down.
 */
export function computeLayout(
  W: number,
  H: number,
  sheet: { top: number; right: number },
  barB: number
): Layout {
  const A = W / H
  const wide = W >= 900 && A > 1.05
  let cx: number, base: number, ph: number
  if (wide) {
    const f0 = sheet.right + 32
    const f1 = W - 84
    ph = Math.min(H * 0.46, (f1 - f0) * 0.9)
    cx = f0 + (f1 - f0) * 0.6
    base = H * 0.85
  } else {
    const top = barB + 8
    const bottom = sheet.top
    ph = Math.min((bottom - top) * 0.72, W * 0.5)
    cx = W * 0.6
    base = bottom - (bottom - top) * 0.05
  }
  return {
    W,
    H,
    A,
    wide,
    cx,
    base,
    ph,
    pu: cx / W,
    pv: 1 - base / H,
    ux: ph / W,
    uy: ph / H,
    sheetR: sheet.right / W,
    barB
  }
}

/** The landscape leaves the panda's silhouette as untouched paper. */
export function keepOut(L: Layout, x: number, y: number): boolean {
  if (!L.ux) return false
  const X = (x - L.pu) / L.ux
  const Y = (y - L.pv) / L.uy
  if (X < -0.9 || X > 0.6 || Y < -0.1 || Y > 1.1) return false
  const e = (a: number, b: number, cx: number, cy: number) =>
    ((X - cx) / a) ** 2 + ((Y - cy) / b) ** 2 < 1
  return (
    e(0.52, 0.42, 0, 0.32) ||
    e(0.33, 0.27, 0, 0.8) ||
    e(0.24, 0.14, -0.52, 0.16)
  )
}

export const TAPER = {
  std: (t: number) =>
    0.3 + 0.7 * Math.min(1, t / 0.1) * Math.min(1, (1 - t) / 0.2 + 0.25),
  flat: (t: number) => 1 + 0.18 * Math.abs(t * 2 - 1) ** 3,
  leaf: (t: number) =>
    Math.max(0.12, Math.sin(Math.PI * Math.sqrt(Math.min(1, t))))
}

export interface Stroke {
  kind: 'stroke'
  pts: Pt[]
  speed: number
  r: number
  ink: number
  cin: number
  green: number
  water: number
  dry: number
  dryGrow: number
  fade: number
  hard: number
  seed: number
  after: number
  tp: keyof typeof TAPER
}

export interface Dot {
  kind: 'dot'
  x: number
  y: number
  r: number
  ink: number
  cin: number
  green: number
  water: number
  hard: number
  after: number
}

/** One part of the panda, revealed along a path the hand follows. */
export interface Part {
  kind: 'part'
  part: number
  dur: number
  path: Pt[]
  after: number
}

export type Cue = 'wake' | 'colophon' | 'seal'

export interface CueItem {
  kind: 'cue'
  cue: Cue
  after: number
}

export type Item = Stroke | Dot | Part | CueItem

export function rng(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) | 0
    let t = Math.imul(a ^ (a >>> 15), 1 | a)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

interface Peak {
  c: number
  h: number
  w: number
}

interface MountainOpts {
  ink: number
  s: number
  cun: number
  moss: number
  mossGreen?: boolean
  folds: number
  wash: number
  green?: number
  x0: number
  x1: number
}

interface BambooOpts {
  r: number
  ink: number
  g: number
  s: number
  lean: number
}

export function buildScene(seed: number, L: Layout): Item[] {
  const R = rng(seed)
  const rr = (a: number, b: number) => a + (b - a) * R()
  const pick = <T>(a: T[]): T => a[Math.floor(R() * a.length)]!
  const { A, wide, pu, pv, ux, uy, sheetR } = L
  const Wx = (d: number) => d / A
  const Q: Item[] = []
  const S = (pts: Pt[], o: Partial<Omit<Stroke, 'kind' | 'pts'>>) =>
    Q.push({
      kind: 'stroke',
      pts,
      speed: 1,
      r: 0.005,
      ink: 1,
      cin: 0,
      green: 0,
      water: 0.3,
      dry: 0,
      dryGrow: 0,
      fade: 0,
      hard: 0,
      seed: rr(0, 900),
      after: 0.06,
      tp: 'std',
      ...o
    })
  const D = (x: number, y: number, o: Partial<Omit<Dot, 'kind'>>) =>
    Q.push({
      kind: 'dot',
      x,
      y,
      r: 0.005,
      ink: 1,
      cin: 0,
      green: 0,
      water: 0.4,
      hard: 0,
      after: 0.03,
      ...o
    })
  const C = (cue: Cue, after = 0) => Q.push({ kind: 'cue', cue, after })
  const pan = (X: number, Y: number): Pt => [pu + X * ux, pv + Y * uy]
  const P = (part: number, dur: number, path: Pt[], after = 0.08) =>
    Q.push({
      kind: 'part',
      part,
      dur,
      path: path.map((p) => pan(p[0], p[1])),
      after
    })
  const wave = () => {
    const k = [rr(0, 6.28), rr(0, 6.28), rr(0, 6.28)] as const
    return (x: number) => {
      x *= A
      return (
        Math.sin(x * 14 + k[0]) * 0.5 +
        Math.sin(x * 36 + k[1]) * 0.3 +
        Math.sin(x * 82 + k[2]) * 0.2
      )
    }
  }
  const range = (peaks: Peak[], base: number, nAmp: number) => {
    const n = wave()
    return (x: number) => {
      let h = 0
      for (const p of peaks) {
        const u = (Math.abs(x - p.c) * A) / p.w
        h = Math.max(h, p.h * Math.exp(-Math.pow(u, 1.5)))
      }
      return base + h + nAmp * n(x) * Math.min(1, h / 0.05)
    }
  }
  const runs = (
    f: (x: number) => number,
    base: number,
    x0: number,
    x1: number,
    dx: number,
    th: number
  ) => {
    const out: Pt[][] = []
    let cur: Pt[] | null = null
    for (let x = x0; x <= x1 + 1e-9; x += dx) {
      const y = f(x)
      if (y - base > th) {
        if (!cur) {
          cur = []
          out.push(cur)
        }
        cur.push([x, y])
      } else cur = null
    }
    return out.filter((r) => r.length > 2)
  }

  function mountain(f: (x: number) => number, base: number, o: MountainOpts) {
    const rs = runs(f, base, o.x0, o.x1, 0.005, 0.012)
    if (!rs.length) return
    for (const run of rs)
      S(run, {
        r: 0.0042 * o.s,
        ink: 1.05 * o.ink,
        water: 0.25,
        dry: 0.05,
        dryGrow: 0.5,
        fade: 0.35,
        speed: 0.7
      })
    const pk: Pt[] = []
    for (const run of rs)
      for (let i = 2; i < run.length - 2; i++) {
        const p = run[i]!
        if (
          p[1] > run[i - 1]![1] &&
          p[1] >= run[i + 1]![1] &&
          p[1] - base > 0.06
        )
          pk.push(p)
      }
    for (const p of pk.slice(0, o.folds)) {
      const side = R() < 0.5 ? -1 : 1
      const len = (p[1] - base) * rr(0.45, 0.75)
      const pts: Pt[] = []
      let x = p[0]
      let y = p[1] - 0.004
      for (let i = 0; i <= 8; i++) {
        pts.push([x, y])
        y -= len / 8
        x += Wx(side * rr(0.002, 0.009))
      }
      S(pts, {
        r: 0.0036 * o.s,
        ink: 0.8 * o.ink,
        water: 0.2,
        dry: 0.25,
        dryGrow: 0.5,
        fade: 0.5,
        speed: 0.8,
        after: 0.04
      })
    }
    for (let i = 0; i < o.cun; i++) {
      const run = pick(rs)
      const q = run[Math.floor(rr(0.05, 0.95) * run.length)]!
      const top = q[1]
      const y = top - rr(0.008, Math.max(0.012, (top - base) * 0.6))
      const x = q[0]
      const slope = (f(x + 0.005) - f(x - 0.005)) / 0.01
      const side = slope > 0 ? -1 : 1
      const len = rr(0.02, 0.045)
      const pts: Pt[] = [[x, y]]
      for (let k = 1; k <= 3; k++)
        pts.push([
          x + Wx(((side * len * k) / 3) * 0.55 + rr(-0.002, 0.002)),
          y - (len * k) / 3
        ])
      S(pts, {
        r: 0.0026 * o.s,
        ink: 0.6 * o.ink,
        water: 0.12,
        dry: 0.45,
        dryGrow: 0.3,
        speed: 1.4,
        after: 0.012
      })
    }
    for (let j = 0; j < o.wash; j++) {
      const off = 0.008 + j * 0.018
      const ij = 0.15 * o.ink * (1 - j / (o.wash + 0.5))
      for (const run of runs((x) => f(x) - off, base, o.x0, o.x1, 0.02, 0.006))
        S(run, { r: 0.02, ink: ij, water: 0.8, speed: 3.4, after: 0.02 })
    }
    // 青绿: mineral green laid over the ink on the upper slopes
    for (let j = 0; j < (o.green ? 3 : 0); j++) {
      const off = 0.01 + j * 0.012
      for (const run of runs((x) => f(x) - off, base, o.x0, o.x1, 0.02, 0.03))
        S(run, {
          r: 0.013,
          ink: 0,
          green: (o.green ?? 0) * (1 - j / 4),
          water: 0.55,
          speed: 3.4,
          after: 0.02
        })
    }
    for (let i = 0; i < o.moss; i++) {
      const run = pick(rs)
      const q = run[Math.floor(rr(0.1, 0.9) * run.length)]!
      D(q[0] + Wx(rr(-0.004, 0.004)), q[1] + rr(-0.002, 0.006), {
        r: rr(0.003, 0.0055) * o.s,
        ink: rr(0.9, 1.4) * o.ink,
        green: o.mossGreen ? rr(0.5, 1) : 0,
        water: 0.3,
        after: 0.04
      })
    }
  }

  // 墨竹: segmented stalks, dark node marks, drooping leaves in 个 clusters
  function bamboo(x: number, y0: number, y1: number, o: BambooOpts) {
    const n = Math.max(3, Math.round((y1 - y0) / rr(0.075, 0.1)))
    const seg = (y1 - y0) / n
    const gap = 0.0055
    const xs = (y: number) => x + Wx(o.lean * (y - y0))
    const nodes: Pt[] = []
    for (let i = 0; i < n; i++) {
      const a = y0 + i * seg + gap
      const b = y0 + (i + 1) * seg - gap
      const m = (a + b) / 2
      S(
        [
          [xs(a), a],
          [xs(m) + Wx(rr(-0.0008, 0.0008)), m],
          [xs(b), b]
        ],
        {
          r: o.r,
          ink: 0.5 * o.ink,
          green: 0.7 * o.g,
          water: 0.35,
          dry: 0.15,
          dryGrow: 0.35,
          speed: 1.1,
          tp: 'flat',
          after: 0.015
        }
      )
      nodes.push([xs(b + gap), b + gap])
    }
    for (const [nx, ny] of nodes.slice(0, -1))
      S(
        [
          [nx - Wx(o.r * 1.7), ny + 0.001],
          [nx + Wx(o.r * 1.7), ny - 0.001]
        ],
        {
          r: o.r * 0.32,
          ink: 1.25 * o.ink,
          water: 0.15,
          speed: 0.5,
          tp: 'flat',
          after: 0.015
        }
      )
    const tips: Pt[] = [[xs(y1), y1 + gap]]
    for (const [nx, ny] of nodes.slice(Math.floor(n * 0.45), -1)) {
      if (R() < 0.35) continue
      const side = R() < 0.5 ? -1 : 1
      const len = rr(0.035, 0.07)
      const a = side * rr(0.5, 0.9)
      const pts: Pt[] = []
      for (let k = 0; k <= 4; k++) {
        const t = k / 4
        pts.push([nx + Wx(Math.sin(a) * len * t), ny + Math.cos(a) * len * t])
      }
      S(pts, {
        r: o.r * 0.28,
        ink: 0.9 * o.ink,
        water: 0.15,
        dry: 0.2,
        speed: 0.9,
        tp: 'flat',
        after: 0.015
      })
      tips.push(pts[4]!)
    }
    for (const [tx, ty] of tips) {
      const k = Math.floor(rr(3, 6))
      const lean = R() < 0.5 ? -1 : 1
      for (let i = 0; i < k; i++) {
        const a = -Math.PI / 2 + lean * rr(0.15, 1.05) + rr(-0.3, 0.3)
        const len = rr(0.045, 0.08) * o.s
        const pts: Pt[] = []
        for (let j = 0; j <= 6; j++) {
          const t = j / 6
          pts.push([
            tx + Wx(Math.cos(a) * len * t),
            ty + Math.sin(a) * len * t - 0.18 * len * t * t
          ])
        }
        S(pts, {
          r: rr(0.0065, 0.0095) * o.s,
          ink: rr(0.85, 1.2) * o.ink,
          green: rr(0.3, 0.7) * o.g,
          water: 0.25,
          dry: 0.1,
          dryGrow: 0.5,
          speed: 0.9,
          tp: 'leaf',
          after: 0.015
        })
      }
    }
  }

  const top = 1 - (L.barB + 10) / L.H
  // 远山 far range, pale, running under the sheet on the left
  const farBase = pv + uy * rr(0.95, 1.1)
  const farPeaks: Peak[] = []
  for (let i = 0; i < 6; i++)
    farPeaks.push({ c: rr(-0.05, 1.05), h: rr(0.05, 0.13), w: rr(0.08, 0.15) })
  const far = range(farPeaks, farBase, 0.005)
  for (const run of runs(far, farBase, 0, 1, 0.007, 0.008))
    S(run, { r: 0.004, ink: 0.2, water: 0.8, speed: 1.1 })
  for (let j = 0; j < 2; j++) {
    const off = 0.012 + j * 0.022
    for (const run of runs((x) => far(x) - off, farBase, 0, 1, 0.02, 0.01))
      S(run, {
        r: 0.02,
        ink: 0.065 * (1 - j / 2.5),
        water: 0.75,
        speed: 3.6,
        after: 0.02
      })
  }
  // Her first, right after a few pale hills: head, ears, eye patches, then
  // the body, the black coat and the bamboo, then 点睛. Once her eyes are in
  // she wakes and watches the brush paint the rest of the scene.
  P(
    11,
    0.4,
    [
      [-0.6, 0],
      [0.5, 0]
    ],
    0.02
  )
  P(4, 1.1, [
    [0.24, 0.85],
    [0.1, 0.97],
    [-0.1, 0.97],
    [-0.24, 0.85],
    [-0.28, 0.72],
    [-0.15, 0.6],
    [0, 0.59],
    [0.15, 0.6],
    [0.28, 0.72]
  ])
  P(3, 0.45, [
    [-0.212, 0.94],
    [0.212, 0.94]
  ])
  P(5, 0.6, [
    [-0.083, 0.772],
    [0.083, 0.772]
  ])
  P(6, 0.4, [
    [0, 0.683],
    [0, 0.645]
  ])
  P(0, 1.2, [
    [-0.36, 0.66],
    [-0.44, 0.45],
    [-0.4, 0.15],
    [-0.25, 0.01],
    [0, -0.01],
    [0.25, 0.01],
    [0.4, 0.15],
    [0.44, 0.45],
    [0.36, 0.66]
  ])
  P(1, 0.8, [
    [-0.2, 0.17],
    [-0.36, 0.09],
    [0.2, 0.17],
    [0.36, 0.09]
  ])
  P(2, 0.45, [
    [-0.3, 0.7],
    [-0.44, 0.6],
    [0.3, 0.7],
    [0.44, 0.6]
  ])
  P(7, 0.7, [
    [0.02, 0.652],
    [-0.47, -0.01]
  ])
  P(8, 0.7, [
    [-0.5, 0.16],
    [-0.66, 0.02],
    [-0.7, 0.12]
  ])
  P(
    9,
    0.9,
    [
      [-0.32, 0.5],
      [-0.3, 0.3],
      [0.34, 0.5],
      [0.33, 0.33],
      [0.09, 0.59],
      [-0.28, 0.25]
    ],
    0.35
  )
  P(
    10,
    0.35,
    [
      [-0.08, 0.79],
      [0.08, 0.79]
    ],
    0.1
  )
  C('wake', 0.5)
  // 竹 a stand of bamboo between the sheet and her
  const gx = wide
    ? [sheetR - 0.015, sheetR + Wx(0.035), sheetR + Wx(0.07)]
    : [0.06, 0.14, 0.22]
  const bs = wide ? 1 : 0.72
  gx.forEach((x, i) =>
    bamboo(
      x,
      pv - uy * 0.06,
      Math.min(top - 0.02, pv + uy * rr(1.35, 1.75) - i * 0.03),
      {
        r: rr(0.0055, 0.007) * bs,
        ink: i === 1 ? 0.75 : 1,
        g: 1,
        s: bs,
        lean: rr(-0.05, 0.05)
      }
    )
  )
  // 近岸 the bank she sits on
  const bank = range(
    [
      { c: pu, h: uy * 0.05, w: 0.3 },
      { c: pu + ux * rr(0.9, 1.2), h: uy * rr(0.1, 0.16), w: 0.08 },
      { c: pu - ux * rr(0.9, 1.2), h: uy * 0.06, w: 0.12 }
    ],
    pv - uy * 0.035,
    0.004
  )
  mountain(bank, pv - uy * 0.035, {
    ink: 1,
    s: 1.1,
    cun: 6,
    moss: 10,
    mossGreen: true,
    folds: 1,
    wash: 4,
    x0: wide ? sheetR - 0.03 : 0,
    x1: 1
  })
  // 主峰 main peak behind her
  const c0 = pu + ux * (wide ? rr(0.2, 0.38) : rr(-0.1, 0.1))
  const mBase = pv + uy * 0.45
  const mH = Math.min(top - 0.04 - mBase, rr(0.34, 0.42))
  const main = range(
    [
      { c: c0, h: mH, w: rr(0.1, 0.13) },
      { c: c0 + Wx(rr(0.1, 0.14)), h: mH * rr(0.5, 0.62), w: rr(0.07, 0.09) },
      { c: c0 - Wx(rr(0.13, 0.17)), h: mH * rr(0.42, 0.55), w: rr(0.08, 0.1) }
    ],
    mBase,
    0.011
  )
  mountain(main, mBase, {
    ink: 0.9,
    s: 1,
    cun: 16,
    moss: 8,
    folds: 4,
    wash: 4,
    green: 0.2,
    x0: wide ? sheetR - 0.02 : 0,
    x1: 1
  })
  // 次峰 a second peak, seen through the tracing paper
  if (wide) {
    const c1 = sheetR * rr(0.3, 0.7)
    const sBase = pv + uy * 0.75
    const second = range(
      [
        { c: c1, h: rr(0.14, 0.2), w: rr(0.09, 0.12) },
        { c: c1 + Wx(0.12), h: rr(0.07, 0.1), w: 0.07 }
      ],
      sBase,
      0.008
    )
    mountain(second, sBase, {
      ink: 0.5,
      s: 0.9,
      cun: 8,
      moss: 4,
      folds: 2,
      wash: 4,
      green: 0.15,
      x0: 0,
      x1: sheetR + 0.05
    })
  }
  // 日 a dot of cinnabar, and birds
  const sunX = wide ? pu - ux * rr(0, 0.18) : rr(0.3, 0.42)
  const sunY = Math.min(top - 0.05, pv + uy * rr(1.35, 1.55))
  D(sunX, sunY, {
    r: 0.036,
    ink: 0,
    cin: 1.25,
    water: 0.15,
    hard: 1,
    after: 0.5
  })
  const bx = sunX - Wx(rr(0.1, 0.14))
  const by = sunY - rr(0.02, 0.05)
  for (let i = 0; i < 2; i++) {
    const x = bx + Wx(i * 0.028 + rr(-0.005, 0.005))
    const y = by + (i % 2) * 0.012
    const s = rr(0.8, 1.1)
    S(
      [
        [x - Wx(0.012 * s), y + 0.004 * s],
        [x - Wx(0.006 * s), y + 0.0035 * s],
        [x, y - 0.002 * s],
        [x + Wx(0.005 * s), y + 0.0035 * s],
        [x + Wx(0.011 * s), y + 0.006 * s]
      ],
      { r: 0.0016, ink: 1.2, water: 0.1, speed: 0.4, after: 0.1 }
    )
  }
  C('colophon', 2.2)
  C('seal', 0.2)
  return Q
}
