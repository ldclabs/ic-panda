/*
  琴: background music for the painting, composed and played live. A
  guqin-like voice plays a tune in a pentatonic mode, grown from two short
  figures made from the seed: open strings (散音), stopped notes with slides
  and vibrato (按音, 吟猱) and bell-like harmonics (泛音). Under it a low xun
  (埙), a clay vessel flute, holds the notes the phrases come home to and
  answers the tune's head, and wind moves through the bamboo. The strings
  are Karplus–Strong plucks rendered on the fly; nothing is downloaded.
*/
import { rng } from './scene'

export interface Music {
  play(): Promise<void>
  pause(): void
  /** A single note in answer to something in the painting. */
  accent(kind: 'poke' | 'seal'): void
  destroy(): void
}

type Kind = 'open' | 'stop' | 'harm' | 'xun'

interface Note {
  midi: number
  kind: Kind
  vel: number
  /** Glide into the note from this many semitones away. */
  slide?: number
  /** Vibrato depth as a fraction of the pitch. */
  vib?: number
  /** How long a held note lasts, in seconds. */
  dur?: number
  /** Seconds after its step that the note begins. */
  delay?: number
}

interface Step {
  notes: Note[]
  /** Seconds until the next step. */
  wait: number
}

/* 正调: the seven strings tuned C D F G A c d, an F-gong pentatonic. */
const PCS = [0, 2, 5, 7, 9]
const OPEN = [36, 38, 41, 43, 45, 48, 50]
const inScale = (m: number) => PCS.includes(((m % 12) + 12) % 12)
const span = (lo: number, hi: number) =>
  Array.from({ length: hi - lo + 1 }, (_, i) => lo + i).filter(inScale)
const mtof = (m: number) => 440 * 2 ** ((m - 69) / 12)

/** The lowest open string, and a harmonic high up, on the mode's tonic. */
const lowTonic = (tonic: number) => OPEN.find((m) => m % 12 === tonic)!
const highTonic = (tonic: number) => span(72, 83).find((m) => m % 12 === tonic)!

/* A figure: scale steps from its first note, and its rhythm in beats. */
interface Motif {
  steps: number[]
  beats: number[]
}

/* One note of a phrase: a degree of the melody's scale and its length. */
interface Ev {
  d: number
  beats: number
}

/* Rhythms a figure of each length can take; each ends on a held note. */
const RHYTHMS: Record<number, number[][]> = {
  3: [
    [1, 1, 2],
    [0.5, 0.5, 2],
    [1.5, 0.5, 2],
    [1, 0.5, 1.5]
  ],
  4: [
    [1, 0.5, 0.5, 2],
    [0.5, 0.5, 1, 2],
    [1, 1, 0.5, 1.5],
    [1.5, 0.5, 1, 1.5]
  ],
  5: [
    [0.5, 0.5, 1, 1, 2],
    [1, 0.5, 0.5, 1, 2],
    [0.5, 0.5, 0.5, 0.5, 2]
  ]
}

/*
  The piece is built from two short figures made from the seed: a head and
  an answer. Each round runs 起承转合: a question that states the head twice
  and rests on the fifth, an answer that brings it home to the tonic, a turn
  in harmonics, and the answer again with the xun taking up the head below
  it. Figures join 鱼咬尾, each starting on the note the last one ended on.
  The answer changes every round and the head every third, so the tune is
  recognisable and still moves on.
*/
function* compose(R: () => number, tonic: number): Generator<Step, never> {
  const rr = (a: number, b: number) => a + (b - a) * R()
  const pick = <T>(a: readonly T[]): T => a[Math.floor(R() * a.length)]!
  const beat = 60 / rr(60, 70)
  const mel = span(48, 81)
  const low = span(45, 57)
  // the index of the note nearest `near` whose pitch class is in `pcs`
  const home = (list: number[], near: number, pcs: number[]) => {
    let best = -1
    for (let k = 0; k < list.length; k++)
      if (
        pcs.includes(list[k]! % 12) &&
        (best < 0 || Math.abs(list[k]! - near) < Math.abs(list[best]! - near))
      )
        best = k
    return Math.max(best, 0)
  }
  const T = home(mel, 62, [tonic])
  // in a pentatonic mode the fifth is three steps up
  const V = T + 3
  const clampD = (d: number) => Math.max(0, Math.min(mel.length - 1, d))
  const at = (d: number) => mel[clampD(d)]!

  // mostly steps, ending near where it began, within four steps overall
  const motif = (n: number): Motif => {
    let steps = [0]
    for (let tries = 0; tries < 30; tries++) {
      steps = [0]
      for (let k = 1; k < n; k++)
        steps.push(steps[k - 1]! + pick([-2, -1, -1, -1, 1, 1, 1, 2]))
      const net = steps[n - 1]!
      if (Math.abs(net) <= 2 && Math.max(...steps) - Math.min(...steps) <= 4)
        break
    }
    return { steps, beats: pick(RHYTHMS[n]!) }
  }
  // 倒影: the same figure upside down
  const invert = (m: Motif): Motif => ({
    steps: m.steps.map((s) => -s),
    beats: m.beats
  })
  const place = (m: Motif, start: number): Ev[] =>
    m.steps.map((s, k) => ({ d: clampD(start + s), beats: m.beats[k]! }))
  // step toward the goal in the octave nearest the melody, then hold it
  const cadence = (from: number, goal: number): Ev[] => {
    for (const g of [goal - 5, goal + 5])
      if (
        g >= 0 &&
        g < mel.length &&
        Math.abs(g - from) < Math.abs(goal - from)
      )
        goal = g
    const out: Ev[] = []
    let d = from
    if (d === goal) out.push({ d: (d = goal + pick([-1, 1])), beats: 1 })
    while (Math.abs(goal - d) > 1 && out.length < 2) {
      d += Math.sign(goal - d)
      out.push({ d, beats: pick([0.5, 1]) })
    }
    out.push({ d: goal, beats: rr(3, 4) })
    return out
  }
  // two figures, the second picking up where the first ended, then home
  const phrase = (
    first: Motif,
    start: number,
    second: Motif,
    goal: number
  ): Ev[] => {
    const fits = (m: Motif, at: number) =>
      m.steps.every((x) => at + x >= 0 && at + x < mel.length)
    const p1 = place(first, start)
    const tail = p1[p1.length - 1]!.d
    // a figure that would run off the qin turns the other way
    const p2 = place(fits(second, tail) ? second : invert(second), tail).slice(
      1
    )
    const end = (p2.length ? p2 : p1)[(p2.length || p1.length) - 1]!.d
    return [...p1, ...p2, ...cadence(end, goal)]
  }

  // 埙: seconds since the start, and when its last breath has faded
  let clock = 0
  let bassFree = 0
  // the xun's note for a pitch: the same pitch class, low, near `near`
  const deep = (m: number, near = 51) => low[home(low, near, [m % 12])]!
  // one long breath under a phrase, on the note it comes home to
  const drone = (goal: number, evs: Ev[]): Note[] => [
    {
      midi: deep(at(goal)),
      kind: 'xun',
      vel: rr(0.065, 0.09),
      dur: Math.max(
        5,
        Math.min(10, evs.reduce((s, e) => s + e.beats, 0) * beat)
      )
    }
  ]
  // or the head of a figure, twice as slow, answering from below
  const echo = (m: Motif, start: number): Note[] => {
    let delay = 0
    let near = 51
    return place(m, start)
      .slice(0, 3)
      .map((ev) => {
        const dur = Math.max(1.6, ev.beats * 2 * beat)
        const midi = deep(at(ev.d), near)
        const n: Note = { midi, kind: 'xun', vel: rr(0.07, 0.09), dur, delay }
        near = midi
        delay += dur * 0.9
        return n
      })
  }

  function* rest(beats: number): Generator<Step> {
    const wait = beats * beat
    clock += wait
    yield { notes: [], wait }
  }

  // A phrase: louder in the middle, slowing into the cadence, with slides
  // and vibrato on stopped notes; the xun comes in on note `xunAt`.
  function* play(
    evs: Ev[],
    kind: 'stop' | 'harm',
    xun: Note[],
    xunAt = 0
  ): Generator<Step> {
    // harmonics sound an octave up, unless that runs off the top of the qin
    const up =
      kind === 'harm' && Math.max(...evs.map((e) => at(e.d))) + 12 <= 86
        ? 12
        : 0
    let prev = -1
    for (let k = 0; k < evs.length; k++) {
      const ev = evs[k]!
      const m = at(ev.d) + up
      const arch = Math.sin((Math.PI * k) / Math.max(1, evs.length - 1))
      const note: Note = {
        midi: m,
        kind,
        vel: (kind === 'harm' ? 0.3 : 0.4) + 0.1 * arch + (k === 0 ? 0.06 : 0)
      }
      if (kind === 'stop') {
        if (prev > 0 && prev !== m && Math.abs(prev - m) <= 5 && R() < 0.25)
          note.slide = prev - m
        if (ev.beats >= 1.5 && R() < 0.7) note.vib = rr(0.003, 0.008)
      }
      prev = m
      const notes = [note]
      if (k === xunAt && xun.length && clock >= bassFree) {
        notes.push(...xun)
        const end = Math.max(...xun.map((n) => (n.delay ?? 0) + (n.dur ?? 0)))
        bassFree = clock + end + rr(0.3, 1.2)
      }
      const wait =
        ev.beats * beat * (k >= evs.length - 2 ? 1.12 : 1) * rr(0.97, 1.03)
      clock += wait
      yield { notes, wait }
    }
  }

  // 散: a few open strings, slow, ending on the tonic, the xun under them
  const tonicString = OPEN.indexOf(lowTonic(tonic))
  function* san(): Generator<Step> {
    const n = 2 + Math.floor(R() * 3)
    let s = 2 + Math.floor(R() * 5)
    for (let k = 0; k < n; k++) {
      const last = k === n - 1
      if (last) s = tonicString
      else
        s = Math.max(
          0,
          Math.min(OPEN.length - 1, s + pick([-2, -1, -1, 1, 1, 2]))
        )
      const m = OPEN[s]!
      const notes: Note[] = [{ midi: m, kind: 'open', vel: rr(0.5, 0.62) }]
      // 撮: the same note an octave up, plucked together
      if (last && R() < 0.4)
        notes.push({ midi: m + 12, kind: 'stop', vel: 0.3 })
      if (k === 0 && clock >= bassFree) {
        const dur = rr(6, 9)
        notes.push({
          midi: deep(tonic),
          kind: 'xun',
          vel: rr(0.065, 0.09),
          dur
        })
        bassFree = clock + dur + rr(0.3, 1.2)
      }
      const wait = (last ? rr(2.5, 3.5) : pick([1.5, 2, 2, 3])) * beat
      clock += wait
      yield { notes, wait }
    }
  }

  let a = motif(pick([4, 5]))
  let b = motif(pick([3, 4]))
  yield* san()
  yield* rest(rr(1, 2))
  for (let round = 0; ; round++) {
    let s = T + pick([-2, 0, 0, 1])
    let question = phrase(a, s, a, V)
    let answer = phrase(a, s, b, T)
    // keep the tune above the xun and below the top: move both by an octave
    const ds = [...question, ...answer].map((e) => e.d)
    const shift =
      Math.min(...ds) < T - 3 && Math.max(...ds) + 5 < mel.length
        ? 5
        : Math.max(...ds) > T + 7 && Math.min(...ds) >= 5
          ? -5
          : 0
    if (shift) {
      s += shift
      question = question.map((e) => ({ d: e.d + shift, beats: e.beats }))
      answer = answer.map((e) => ({ d: e.d + shift, beats: e.beats }))
    }
    // 起: the question, its head heard twice, resting on the fifth
    yield* play(question, 'stop', drone(V, question))
    yield* rest(rr(1, 1.5))
    // 承: the answer, the same head, coming home to the tonic
    yield* play(answer, 'stop', drone(T, answer))
    yield* rest(rr(1.5, 2.5))
    // 转: the answer again in harmonics, or its figure turned upside down
    const turn = R() < 0.6 ? answer : phrase(b, V, invert(b), pick([V, T]))
    yield* play(turn, 'harm', [])
    yield* rest(rr(1.5, 2.5))
    // 合: the answer once more, the xun taking up the head below it
    yield* play(answer, 'stop', echo(a, s), a.steps.length)
    yield* rest(rr(3, 5))
    if (R() < 0.4) {
      yield* san()
      yield* rest(rr(1, 2))
    }
    // a new answer every round, a new head every third
    b = motif(pick([3, 4]))
    if (round % 3 === 2) a = motif(pick([4, 5]))
  }
}

export function createMusic(seed: number): Music | null {
  if (typeof AudioContext === 'undefined') return null
  let ctx: AudioContext | null = null
  let master: GainNode
  let bus: AudioNode
  let windGain: GainNode
  let windBand: BiquadFilterNode
  let leafGain: GainNode
  let noise: AudioBuffer
  let xunWave: PeriodicWave
  const R = rng(seed ^ 0x9e3779b9)
  // the mode: which degree the phrases come home to (宫 商 徵 羽)
  const tonic = [5, 7, 0, 2][Math.floor(R() * 4)]!
  const gen = compose(R, tonic)
  const cache = new Map<string, { buf: AudioBuffer; rate: number }>()
  let playing = false
  let next = 0
  let nextGust = 0
  let timer = 0
  let sleepTimer = 0

  /* A plucked string: soft noise (a fingertip, not a pick) shaped by where
     the string is plucked, circulating through a lossy delay line. */
  function pluck(midi: number, kind: 'open' | 'stop') {
    const key = kind + midi
    const hit = cache.get(key)
    if (hit) return hit
    const c = ctx!
    const sr = c.sampleRate
    const f = mtof(midi)
    const N = Math.max(8, Math.floor(sr / f - 0.5))
    const rate = f / (sr / (N + 0.5))
    const t60 =
      Math.max(3.5, Math.min(9, 10 - (midi - 36) * 0.14)) *
      (kind === 'open' ? 1 : 0.8)
    const n = Math.floor(Math.min(8, t60 * 0.9) * sr)
    const buf = c.createBuffer(1, n, sr)
    const out = buf.getChannelData(0)
    const ex = new Float32Array(N)
    const a = kind === 'open' ? 0.18 : 0.3
    let lp = 0
    for (let k = 0; k < N; k++) {
      lp += a * (Math.random() * 2 - 1 - lp)
      ex[k] = lp
    }
    // plucked an eighth of the way along, near the bridge
    const pp = Math.max(1, Math.round(N / 8))
    const line = new Float32Array(N)
    let mean = 0
    for (let k = 0; k < N; k++) {
      line[k] = ex[k]! - 0.85 * ex[(k + pp) % N]!
      mean += line[k]!
    }
    mean /= N
    let peak = 1e-6
    for (let k = 0; k < N; k++) {
      line[k] = line[k]! - mean
      peak = Math.max(peak, Math.abs(line[k]!))
    }
    const rho = 10 ** (-3 / (t60 * f))
    let k = 0
    for (let s = 0; s < n; s++) {
      const cur = line[k]!
      const nk = k + 1 === N ? 0 : k + 1
      out[s] = cur / peak
      line[k] = rho * 0.5 * (cur + line[nk]!)
      k = nk
    }
    const attack = Math.floor(sr * 0.0015)
    for (let s = 0; s < attack; s++) out[s] = out[s]! * (s / attack)
    const tail = Math.floor(sr * 0.4)
    for (let s = 0; s < tail; s++) out[n - 1 - s] = out[n - 1 - s]! * (s / tail)
    const v = { buf, rate }
    cache.set(key, v)
    return v
  }

  /* A harmonic: the finger barely touches the string, leaving a glassy tone. */
  function harmonic(midi: number) {
    const key = 'harm' + midi
    const hit = cache.get(key)
    if (hit) return hit
    const c = ctx!
    const sr = c.sampleRate
    const f = mtof(midi)
    const t60 = 3.4
    const n = Math.floor(t60 * sr)
    const buf = c.createBuffer(1, n, sr)
    const out = buf.getChannelData(0)
    const w = (2 * Math.PI * f) / sr
    for (let s = 0; s < n; s++) {
      const t = s / sr
      const env =
        Math.min(1, t / 0.003) *
        Math.exp((-6.91 * t) / t60) *
        Math.min(1, (n - s) / (sr * 0.2))
      out[s] =
        0.7 *
        env *
        (Math.sin(w * s) +
          0.16 * Math.exp(-t * 2.5) * Math.sin(2 * w * s) +
          0.05 * Math.exp(-t * 5) * Math.sin(3 * w * s))
    }
    const v = { buf, rate: 1 }
    cache.set(key, v)
    return v
  }

  /* 埙: a clay vessel flute, almost a pure tone with breath around it. It
     scoops up into the note, swells and fades, and a slow vibrato comes in
     as the breath settles. */
  function xun(note: Note, t: number) {
    const c = ctx!
    const f = mtof(note.midi)
    const dur = note.dur ?? 6
    const end = t + dur
    // long breaths swell slowly, the notes of a line more quickly
    const att = Math.min(1, dur * 0.3)
    const osc = c.createOscillator()
    osc.setPeriodicWave(xunWave)
    osc.frequency.setValueAtTime(f * 0.985, t)
    osc.frequency.exponentialRampToValueAtTime(f, t + Math.min(0.4, att))
    const lfo = c.createOscillator()
    lfo.frequency.value = 4 + R()
    const depth = c.createGain()
    depth.gain.setValueAtTime(0, t)
    depth.gain.linearRampToValueAtTime(f * 0.004, t + Math.min(2.5, dur * 0.6))
    lfo.connect(depth).connect(osc.frequency)
    const env = c.createGain()
    env.gain.setValueAtTime(0, t)
    env.gain.linearRampToValueAtTime(note.vel, t + att)
    env.gain.linearRampToValueAtTime(
      note.vel * 0.75,
      end - Math.min(1.6, dur * 0.4)
    )
    env.gain.linearRampToValueAtTime(0, end)
    // breath: noise around the second harmonic, strongest as the note starts
    const air = c.createBufferSource()
    air.buffer = noise
    air.loop = true
    const band = c.createBiquadFilter()
    band.type = 'bandpass'
    band.frequency.value = f * 2
    band.Q.value = 2.5
    const airGain = c.createGain()
    airGain.gain.setValueAtTime(0, t)
    airGain.gain.linearRampToValueAtTime(note.vel * 2.5, t + att * 0.5)
    airGain.gain.linearRampToValueAtTime(note.vel * 0.6, t + att + 0.2)
    airGain.gain.linearRampToValueAtTime(0, end)
    const pan = c.createStereoPanner()
    pan.pan.value = -0.15
    osc.connect(env).connect(pan)
    air.connect(band).connect(airGain).connect(pan)
    pan.connect(bus)
    osc.onended = () => pan.disconnect()
    for (const node of [osc, lfo]) {
      node.start(t)
      node.stop(end + 0.1)
    }
    air.start(t, R() * 3)
    air.stop(end + 0.1)
  }

  function voice(note: Note, t: number) {
    if (note.kind === 'xun') return xun(note, t)
    const c = ctx!
    const { buf, rate } =
      note.kind === 'harm' ? harmonic(note.midi) : pluck(note.midi, note.kind)
    const src = c.createBufferSource()
    src.buffer = buf
    if (note.slide || note.vib) {
      // 吟猱 and 上下: one pitch curve, a glide in, then a slow vibrato that fades
      const dur = Math.min(buf.duration, 3)
      const curve = new Float32Array(Math.ceil(dur * 120))
      for (let k = 0; k < curve.length; k++) {
        const tt = k / 120
        const g = Math.min(1, Math.max(0, (tt - 0.05) / 0.3))
        const semis = (note.slide ?? 0) * (1 - g * g * (3 - 2 * g))
        const on = Math.min(1, Math.max(0, (tt - 0.3) / 0.5))
        const vib =
          1 +
          (note.vib ?? 0) *
            on *
            Math.exp(-tt * 0.7) *
            Math.sin(2 * Math.PI * 4.5 * tt)
        curve[k] = rate * 2 ** (semis / 12) * vib
      }
      src.playbackRate.setValueCurveAtTime(curve, t, dur)
    } else src.playbackRate.value = rate
    const g = c.createGain()
    g.gain.value = note.vel
    const pan = c.createStereoPanner()
    pan.pan.value = (note.midi - 60) / 60 + (R() - 0.5) * 0.2
    src.connect(g).connect(pan).connect(bus)
    src.onended = () => pan.disconnect()
    src.start(t)
  }

  function impulse(c: AudioContext, sec: number) {
    const sr = c.sampleRate
    const n = Math.floor(sec * sr)
    const ir = c.createBuffer(2, n, sr)
    for (let ch = 0; ch < 2; ch++) {
      const d = ir.getChannelData(ch)
      let lp = 0
      for (let s = 0; s < n; s++) {
        const t = s / sr
        // darker as it dies away, like a room of wood and paper
        lp += (0.05 + 0.85 * Math.exp(-t * 1.4)) * (Math.random() * 2 - 1 - lp)
        d[s] = lp * Math.exp((-6.91 * t) / (sec * 0.8)) * Math.min(1, t / 0.015)
      }
    }
    return ir
  }

  function build() {
    const c = new AudioContext({ latencyHint: 'playback' })
    ctx = c
    master = c.createGain()
    master.gain.value = 0
    const comp = c.createDynamicsCompressor()
    comp.threshold.value = -18
    comp.knee.value = 12
    comp.ratio.value = 3
    comp.connect(master).connect(c.destination)
    const verb = c.createConvolver()
    verb.buffer = impulse(c, 3.6)
    const wet = c.createGain()
    wet.gain.value = 0.42
    verb.connect(wet).connect(comp)
    // the instrument: rumble out, a little warmth, the top rounded off
    const hp = c.createBiquadFilter()
    hp.type = 'highpass'
    hp.frequency.value = 50
    const body = c.createBiquadFilter()
    body.type = 'peaking'
    body.frequency.value = 220
    body.Q.value = 0.9
    body.gain.value = 3
    const top = c.createBiquadFilter()
    top.type = 'highshelf'
    top.frequency.value = 3200
    top.gain.value = -6
    hp.connect(body).connect(top)
    const dry = c.createGain()
    dry.gain.value = 0.8
    top.connect(dry).connect(comp)
    top.connect(verb)
    bus = hp
    xunWave = c.createPeriodicWave([0, 0, 0, 0, 0], [0, 1, 0.22, 0.06, 0.02])
    // wind in the bamboo: white noise, band-passed, in slow gusts
    const sr = c.sampleRate
    noise = c.createBuffer(1, sr * 4, sr)
    const nd = noise.getChannelData(0)
    for (let s = 0; s < nd.length; s++) nd[s] = Math.random() * 2 - 1
    const src = c.createBufferSource()
    src.buffer = noise
    src.loop = true
    windBand = c.createBiquadFilter()
    windBand.type = 'bandpass'
    windBand.frequency.value = 450
    windBand.Q.value = 0.7
    windGain = c.createGain()
    windGain.gain.value = 0.006
    const leaves = c.createBiquadFilter()
    leaves.type = 'highpass'
    leaves.frequency.value = 2600
    leafGain = c.createGain()
    leafGain.gain.value = 0.001
    src.connect(windBand).connect(windGain).connect(comp)
    src.connect(leaves).connect(leafGain).connect(comp)
    windGain.connect(verb)
    src.start()
    if (import.meta.env.DEV)
      (window as unknown as { __music: unknown }).__music = { ctx: c, master }
  }

  function gust(now: number) {
    if (now < nextGust) return
    const rise = 2 + R() * 3
    const fall = 3 + R() * 4
    const peak = 0.03 + R() * 0.04
    windGain.gain.setTargetAtTime(peak, now, rise / 3)
    windGain.gain.setTargetAtTime(0.006, now + rise, fall / 3)
    leafGain.gain.setTargetAtTime(peak * 0.25, now + rise * 0.6, rise / 3)
    leafGain.gain.setTargetAtTime(0.001, now + rise, fall / 4)
    windBand.frequency.setTargetAtTime(320 + R() * 380, now, rise)
    nextGust = now + rise + fall + 6 + R() * 14
  }

  function tick() {
    const c = ctx!
    const now = c.currentTime
    if (next < now) next = now + 0.1
    while (next < now + 1.5) {
      const step = gen.next().value
      for (const n of step.notes) voice(n, next + (n.delay ?? 0))
      next += step.wait
    }
    gust(now)
  }

  const onVisibility = () => {
    if (!ctx || !playing) return
    if (document.hidden) void ctx.suspend()
    else void ctx.resume()
  }
  document.addEventListener('visibilitychange', onVisibility)

  return {
    async play() {
      // The context is created inside the click that asked for sound.
      if (!ctx) build()
      const c = ctx!
      playing = true
      clearTimeout(sleepTimer)
      await c.resume()
      if (!playing) return
      const now = c.currentTime
      master.gain.cancelScheduledValues(now)
      master.gain.setTargetAtTime(1.1, now, 0.9)
      if (!timer) {
        tick()
        timer = window.setInterval(tick, 250)
      }
    },
    pause() {
      playing = false
      clearInterval(timer)
      timer = 0
      if (!ctx) return
      const c = ctx
      master.gain.cancelScheduledValues(c.currentTime)
      master.gain.setTargetAtTime(0, c.currentTime, 0.3)
      sleepTimer = window.setTimeout(() => {
        if (!playing) void c.suspend()
      }, 1500)
    },
    accent(kind) {
      if (!ctx || !playing) return
      const t = ctx.currentTime + 0.02
      if (kind === 'seal')
        voice({ midi: lowTonic(tonic), kind: 'open', vel: 0.55 }, t)
      else voice({ midi: highTonic(tonic), kind: 'harm', vel: 0.32 }, t)
    },
    destroy() {
      playing = false
      clearInterval(timer)
      clearTimeout(sleepTimer)
      document.removeEventListener('visibilitychange', onVisibility)
      void ctx?.close()
      ctx = null
    }
  }
}
