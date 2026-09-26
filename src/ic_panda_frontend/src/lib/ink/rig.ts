/*
  The panda rig. Every moving part is a damped spring chasing a target, so
  motion has weight: eyes lead, the head follows, ears and leaves lag behind.
  Her default is awake and eating. A moving cursor (or, while the scene is
  being painted, the brush) only draws her eyes and head; a poke is the one
  thing that stops her chewing for a moment.
*/
import type { Layout } from './scene'

interface Spring {
  x: number
  v: number
}

const spring = (): Spring => ({ x: 0, v: 0 })

function pull(s: Spring, target: number, dt: number, k: number, zeta = 1) {
  s.v += (k * (target - s.x) - 2 * Math.sqrt(k) * zeta * s.v) * dt
  s.x += s.v * dt
}

export const clamp01 = (x: number) => Math.max(0, Math.min(1, x))
const ease = (x: number) => {
  x = clamp01(x)
  return x * x * (3 - 2 * x)
}

export type Rig = ReturnType<typeof createRig>

export function createRig() {
  return {
    /** Reveal progress of each panda part, see the panda shader. */
    rev: new Float32Array(12),
    alive: false,
    alpha: 1,
    t: 0,
    turn: spring(),
    nod: spring(),
    tilt: spring(),
    hx: spring(),
    hy: spring(),
    gx: spring(),
    gy: spring(),
    pawX: spring(),
    pawY: spring(),
    jaw: spring(),
    earL: spring(),
    earR: spring(),
    leaf: spring(),
    open: 1,
    wide: 0,
    breath: 0,
    rustle: 0,
    blinkAt: 2,
    blinkT: -1,
    nextTwitch: 3,
    bit: false,
    curious: -1,
    curiousSide: 1,
    glanceCycle: -1,
    glance: [0, 0] as [number, number]
  }
}

export function resetRig(r: Rig) {
  r.rev.fill(0)
  r.alive = false
  r.curious = -1
  r.wide = 0
}

function kickEars(r: Rig, l: number, rt: number) {
  r.earL.v += l
  r.earR.v += rt
}

export function wakeRig(r: Rig) {
  r.alive = true
  r.wide = 1
  kickEars(r, 3, -3)
  r.blinkAt = r.t + 0.4
}

/** A poke: she stops chewing, ears up, looks at you and slowly tilts her head. */
export function pokeRig(r: Rig): boolean {
  if (!r.alive) return false
  r.curious = 0
  r.curiousSide = Math.random() < 0.5 ? -1 : 1
  r.wide = 1
  kickEars(r, 2.5, -2.5)
  r.leaf.v += 1
  return true
}

/** Screen point → gaze direction from her face, unit length at most. */
export function gazeAt(L: Layout, x: number, y: number): [number, number] {
  const dx = (x - L.cx) / L.ph
  const dy = -(y - (L.base - L.ph * 0.78)) / L.ph
  const m = Math.max(Math.hypot(dx, dy), 0.8)
  return [dx / m, dy / m]
}

export function hitPanda(L: Layout | null, x: number, y: number): boolean {
  if (!L?.ph) return false
  const X = (x - L.cx) / L.ph
  const Y = (L.base - y) / L.ph
  return (
    (X / 0.42) ** 2 + ((Y - 0.33) / 0.35) ** 2 < 1 ||
    (X / 0.3) ** 2 + ((Y - 0.78) / 0.22) ** 2 < 1
  )
}

export function rigTick(
  r: Rig,
  dt: number,
  target: [number, number] | null,
  reduced: boolean
) {
  r.t += dt
  const t = r.t
  if (!r.alive) target = null

  let turn = 0,
    nod = 0,
    tilt = -0.04,
    hx = 0,
    hy = 0,
    gx = 0,
    gy = 0
  let pawX = 0,
    pawY = 0,
    jaw = 0,
    open = 1,
    ears = 0
  if (r.alive && r.curious < 0 && !reduced) {
    // bite the end of the stalk, tear, chew with the jaw (the ears ride
    // along), pause and swallow; the paw pulls against the head on the tear
    const period = 6
    const ph = (t % period) / period
    if (ph < 0.12) {
      const a = ease(ph / 0.12)
      pawX = -0.012 * a
      pawY = 0.02 * a
      jaw = 0.75 * a
      turn = 0.12 * a
      tilt -= 0.05 * a
      nod = -0.05 * a
      r.bit = false
    } else if (ph < 0.2) {
      const a = Math.sin(((ph - 0.12) / 0.08) * Math.PI)
      pawX = -0.012 + 0.014 * a
      pawY = 0.02 - 0.055 * a
      jaw = 0.75 * (1 - a)
      turn = 0.12 - 0.18 * a
      nod = -0.05 + 0.14 * a
      tilt -= 0.05 * (1 - a)
      if (!r.bit && a > 0.5) {
        r.bit = true
        r.leaf.v -= 1.6
      }
    } else if (ph < 0.86) {
      const c = (ph - 0.2) * period
      jaw = 0.4 + 0.4 * Math.sin(c * Math.PI * 2 * 1.6 - Math.PI / 2)
      hy = -0.004 * jaw
      ears = 0.35 * jaw
      pawY = -0.004 * jaw
    } else {
      // between bites, now and then a glance away
      const n = Math.floor(t / period)
      if (r.glanceCycle !== n) {
        r.glanceCycle = n
        r.glance =
          Math.random() < 0.5
            ? [Math.random() * 1.4 - 0.7, Math.random() * 0.6 - 0.2]
            : [0, 0]
      }
      ;[gx, gy] = r.glance
    }
  }
  if (target) {
    gx = target[0]
    gy = target[1]
    turn += gx * 0.45
    nod += gy * 0.35
    tilt -= gx * 0.04
  }
  if (r.curious >= 0) {
    r.curious += dt
    const c = r.curious
    const a = ease((c - 0.5) / 1.1) * (1 - ease((c - 3.4) / 1))
    if (!target) {
      gx = 0
      gy = 0.05
    }
    tilt += r.curiousSide * 0.24 * a
    nod += 0.1 * a
    hx = -r.curiousSide * 0.01 * a
    if (c > 4.5) r.curious = -1
  }

  // blink
  if (r.alive) {
    if (r.blinkT < 0 && t > r.blinkAt) r.blinkT = 0
    if (r.blinkT >= 0) {
      r.blinkT += dt
      if (r.blinkT < 0.2) open = Math.abs(r.blinkT - 0.1) / 0.1
      else {
        r.blinkT = -1
        r.blinkAt = t + (Math.random() < 0.2 ? 0.35 : 3 + Math.random() * 4)
      }
    }
  }
  r.open += (open - r.open) * (open < r.open ? 1 : 1 - Math.exp(-dt * 14))
  r.wide += (0 - r.wide) * (1 - Math.exp(-dt * 0.9))

  pull(r.gx, gx, dt, 110)
  pull(r.gy, gy, dt, 110)
  pull(r.turn, turn, dt, 12, 0.9)
  pull(r.nod, nod, dt, 12, 0.9)
  pull(r.tilt, tilt, dt, 9, 0.85)
  pull(r.hx, hx, dt, 14)
  pull(r.hy, hy, dt, 60)
  pull(r.pawX, pawX, dt, 20, 0.95)
  pull(r.pawY, pawY, dt, 20, 0.95)
  pull(r.jaw, jaw, dt, 180)
  // ears: loose springs, the odd twitch, a wiggle with every chew
  if (r.alive && !reduced && t > r.nextTwitch) {
    kickEars(r, Math.random() < 0.5 ? 1.6 : 0, Math.random() < 0.5 ? -1.6 : 0)
    r.nextTwitch = t + 4 + Math.random() * 7
  }
  pull(r.earL, ears, dt, 45, 0.3)
  pull(r.earR, -ears, dt, 45, 0.3)
  // leaves trail the stalk and sway a little in the air
  pull(r.leaf, 0, dt, 14, 0.25)
  r.leaf.v -= r.pawY.v * 10 * dt
  const still = reduced || !r.alive
  r.rustle =
    r.leaf.x +
    (still ? 0 : 0.035 * Math.sin(t * 0.9) + 0.018 * Math.sin(t * 2.1))
  r.breath = still ? 0 : 0.01 * Math.sin(t * 1.3)
}
