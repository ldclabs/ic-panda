<script lang="ts">
  import { onMount } from 'svelte'
  import { flip } from 'svelte/animate'
  import { fly } from 'svelte/transition'
  import { locale, t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { glyphs, type GlyphName } from './glyphs'
  import { ASKERS, HOSTS, ROOMS, renderHeroArt } from './hero-art.js'

  // The gatehouse at work, with sample data: traffic passes the gap into the
  // rooms or out to the agent network, and requests stop at the gem until the
  // visitor decides in the approval window. Nothing here approves on its own.
  type Asker = (typeof ASKERS)[number]
  type Room = (typeof ROOMS)[number]
  type Kind = 'signature' | 'signin' | 'payment'
  type Decision = 'approve' | 'reject' | 'expire'
  type Request = {
    kind: Kind
    from: Asker
    via: string
    title: string
    params?: Record<string, string>
    effect: string
    approved: string
    rejected: string
  }
  type Entry = {
    id: number
    time: string
    glyph: GlyphName
    from?: Asker
    to?: string
    note: string
    tone?: 'ok' | 'no'
  }

  const AGENT = 'release-assistant'
  // Amounts keep their order inside right-to-left sentences.
  const ltr = (text: string) =>
    String.fromCharCode(0x2066) + text + String.fromCharCode(0x2069)
  // A sample origin; example.com is reserved for examples.
  const SITE = 'example.com'
  const NAMES: Record<Asker, string> = {
    you: 'You',
    teammate: 'Mei',
    agent: AGENT,
    website: 'Website'
  }
  const TRANSLATED = new Set<Asker>(['you', 'website'])
  const HOST_NAMES = ['Agent Mail', 'Agent Discourse', 'Agent Knowledge']
  const HOST_GLYPHS: GlyphName[] = ['mail', 'discuss', 'book']
  const KIND_LABEL: Record<Kind, string> = {
    signature: 'Signature',
    signin: 'Sign-in',
    payment: 'Payment'
  }
  const REQUESTS: Request[] = [
    {
      kind: 'signature',
      from: 'agent',
      via: `agent:${AGENT}`,
      title: 'Sign the release statement',
      effect: 'Approving signs this statement with your account key.',
      approved: 'Signature generated',
      rejected: 'Rejected. Nothing was signed.'
    },
    {
      kind: 'signin',
      from: 'website',
      via: `connect:${SITE}`,
      title: 'Sign in to {site}',
      params: { site: SITE },
      effect: 'Approving proves to the site that it is you.',
      approved: 'The site received the result',
      rejected: 'The site is told no; nothing is signed'
    },
    {
      kind: 'payment',
      from: 'agent',
      via: `agent:${AGENT}`,
      title: 'Pay {amount} from the team wallet',
      params: { amount: ltr('0.4 BTC') },
      effect: 'Approving adds your vote. The wallet’s policy decides the rest.',
      approved: 'Your vote is recorded.',
      rejected: 'Rejected. Nothing was paid.'
    }
  ]
  const EXPIRY = 15000
  const APPROVE_DELAY = 1200
  const HOLD = 1000

  const id = $props.id()
  const prefix = `hero-${id}`.replace(/[^a-zA-Z0-9-]/g, '')
  let root: HTMLElement
  let stage: HTMLElement
  let enabled = $state(true)
  let visible = $state(false)
  let reduced = $state(true)
  let foreground = $state(true)
  let narrow = $state(false)
  const playing = $derived(enabled && visible && foreground && !reduced)

  let entries = $state<Entry[]>([])
  let request = $state.raw<Request | null>(null)
  let outcome = $state<{ text: string; tone: 'ok' | 'no' } | null>(null)
  let left = $state(1)
  let ready = $state(false)
  let holding = $state(false)
  let waiting = $state(false)

  const artwork = $derived(
    renderHeroArt({
      prefix,
      labels: {
        people: $t('People and agents'),
        space: $t('Private space'),
        network: $t('Agent network'),
        you: $t('You'),
        desktop: $t('Desktop and terminal'),
        teammate: $t('Teammate'),
        agent: $t('Your agent'),
        website: $t('Website')
      },
      icons: glyphs,
      rtl: $locale === 'ar',
      narrow,
      live: true
    })
  )
  const seconds = $derived(Math.ceil((left * EXPIRY) / 1000))

  // ——— A clock that runs only while the figure plays ———
  let clock = 0
  let timers: { at: number; run: () => void }[] = []
  let tweens: {
    start: number
    ms: number
    step: (p: number) => void
    stop: (() => boolean) | undefined
    done: () => void
  }[] = []

  function frame(now: number, last: number) {
    clock += last ? Math.min(now - last, 50) : 0
    for (const tween of [...tweens]) {
      const p = Math.min(1, (clock - tween.start) / tween.ms)
      tween.step(p)
      if (p === 1 || tween.stop?.()) {
        tweens.splice(tweens.indexOf(tween), 1)
        tween.done()
      }
    }
    for (const timer of timers.filter((timer) => timer.at <= clock)) {
      timers.splice(timers.indexOf(timer), 1)
      timer.run()
    }
  }

  $effect(() => {
    if (!playing) return
    let last = 0
    let handle = requestAnimationFrame(function loop(now) {
      frame(now, last)
      last = now
      handle = requestAnimationFrame(loop)
    })
    return () => cancelAnimationFrame(handle)
  })

  const wait = (ms: number) =>
    new Promise<void>((run) => timers.push({ at: clock + ms, run }))
  const tween = (ms: number, step: (p: number) => void, stop?: () => boolean) =>
    new Promise<void>((done) =>
      tweens.push({ start: clock, ms, step, stop, done })
    )
  const ease = (p: number) =>
    p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2

  // ——— The scene inside {@html} ———
  const find = <T extends Element>(selector: string) =>
    stage.querySelector<T>(selector)!

  function mark(kind: string, value: string, name = 'on') {
    for (const el of stage.querySelectorAll<SVGElement>(`[data-${kind}]`))
      el.classList.toggle(name, el.dataset[kind] === value)
  }

  function clear() {
    for (const kind of ['asker', 'room', 'host']) {
      mark(kind, '')
      mark(kind, '', 'done')
    }
  }

  async function travel(
    route: string,
    layer: 'under' | 'over' | 'request',
    ms: number,
    reverse = false
  ) {
    const path = find<SVGPathElement>(`#${prefix}-${route}`)
    const length = path.getTotalLength()
    const dot = find<SVGGraphicsElement>(
      layer === 'request' ? '.art-request' : `[data-packet="${layer}"]`
    )
    const trail = reverse
      ? null
      : stage.querySelector<SVGPathElement>(`[data-trail="${route}"]`)
    dot.style.opacity = '1'
    if (trail) {
      trail.style.transition = 'none'
      trail.style.opacity = '1'
    }
    await tween(ms, (p) => {
      const e = ease(p)
      const point = path.getPointAtLength(length * (reverse ? 1 - e : e))
      dot.setAttribute('transform', `translate(${point.x} ${point.y})`)
      if (trail) trail.style.strokeDashoffset = String(1 - e)
    })
    dot.style.opacity = '0'
    if (trail) {
      trail.style.transition = ''
      trail.style.opacity = '0'
    }
  }

  function pass() {
    find<SVGCircleElement>('.art-pass').animate(
      [
        { opacity: 0.8, transform: 'scale(1)' },
        { opacity: 0, transform: 'scale(2.1)' }
      ],
      { duration: 700, easing: 'ease-out' }
    )
  }

  let minute = 39
  let serial = 0
  function record(entry: Omit<Entry, 'id' | 'time'>) {
    const time = `09:${String(minute++ % 60).padStart(2, '0')}`
    entries = [{ ...entry, id: serial++, time }, ...entries].slice(0, 3)
  }

  // ——— The story ———
  async function enter(
    from: Asker,
    room: Room,
    note: string,
    glyph: GlyphName
  ) {
    mark('asker', from)
    await travel(`ask-${ASKERS.indexOf(from)}`, 'under', 1500)
    pass()
    await travel(`room-${ROOMS.indexOf(room)}`, 'over', 600)
    mark('room', room)
    mark('asker', '')
    const to = {
      vault: 'Vault',
      files: 'Files',
      channels: 'Channels',
      wallets: 'Team wallets',
      agent: ''
    }[room]
    record({ glyph, from, to, note })
    await wait(1500)
    mark('room', '')
  }

  async function post(host: number) {
    mark('asker', 'agent')
    await travel('ask-2', 'under', 1400)
    pass()
    await travel('room-4', 'over', 500)
    mark('asker', '')
    mark('room', 'agent')
    await wait(500)
    await travel('room-4', 'over', 500, true)
    mark('room', '')
    pass()
    await travel(`host-${host}`, 'under', 1700)
    mark('host', HOSTS[host]!)
    record({
      glyph: HOST_GLYPHS[host]!,
      from: 'agent',
      to: HOST_NAMES[host]!,
      note: 'Signed with its own Agent ID'
    })
    await wait(1600)
    mark('host', '')
  }

  let settle: ((decision: Decision) => void) | null = null
  let readyTimer: ReturnType<typeof setTimeout> | undefined

  function open(next: Request) {
    request = next
    outcome = null
    left = 1
    ready = false
    clearTimeout(readyTimer)
    // As in the approval window: Approve waits a moment after a request appears.
    readyTimer = setTimeout(() => (ready = true), APPROVE_DELAY)
    waiting = true
  }

  function decide(decision: Decision) {
    settle?.(decision)
  }

  async function ask(next: Request) {
    mark('asker', next.from)
    await travel(`ask-${ASKERS.indexOf(next.from)}`, 'request', 1500)
    record({
      glyph: 'time',
      from: next.from,
      to: KIND_LABEL[next.kind],
      note: 'Waiting for your approval'
    })
    open(next)
    let decided: Decision | null = null
    const decision = await Promise.race([
      new Promise<Decision>(
        (resolve) => (settle = (d) => resolve((decided = d)))
      ),
      tween(
        EXPIRY,
        (p) => setLeft(1 - p),
        () => decided !== null
      ).then(() => decided ?? ('expire' as const))
    ])
    settle = null
    await conclude(next, decision)
  }

  function setLeft(value: number) {
    left = value
    find<SVGCircleElement>('.art-timer').style.strokeDashoffset = String(
      1 - value
    )
  }

  async function conclude(next: Request, decision: Decision) {
    waiting = false
    clearTimeout(readyTimer)
    const text =
      decision === 'approve'
        ? next.approved
        : decision === 'reject'
          ? next.rejected
          : 'Expired. Nothing was approved.'
    const tone = decision === 'approve' ? 'ok' : 'no'
    outcome = { text, tone }
    record({ glyph: tone === 'ok' ? 'check' : 'close', note: text, tone })
    if (reduced) return
    mark('asker', '')
    if (decision === 'approve') {
      if (next.kind === 'payment') {
        await travel('room-3', 'over', 700)
        mark('room', 'wallets')
      } else {
        const index = ASKERS.indexOf(next.from)
        await travel(`ask-${index}`, 'under', 1300, true)
        mark('asker', next.from, 'done')
      }
    }
    await wait(2400)
    clear()
    request = null
    outcome = null
  }

  async function story() {
    for (let round = 0; ; round++) {
      await enter('teammate', 'channels', 'Encrypted end to end', 'chat')
      await wait(500)
      await enter('you', 'vault', 'Sealed before it syncs', 'vault')
      await wait(500)
      await enter('agent', 'files', 'Only the folder it was granted', 'folder')
      await wait(500)
      await post(round % 3)
      await wait(700)
      await ask(REQUESTS[round % 3]!)
      await wait(1200)
    }
  }

  // With reduced motion the scene stays still: a request waits, and each
  // decision brings the next one.
  let still = 0
  function stillRequest() {
    const next = REQUESTS[still++ % REQUESTS.length]!
    open(next)
    settle = (decision) => {
      settle = null
      void conclude(next, decision).then(() =>
        setTimeout(() => {
          outcome = null
          if (reduced) stillRequest()
        }, 2400)
      )
    }
  }

  // ——— Hold to approve, for payments ———
  let holdTimer: ReturnType<typeof setTimeout> | undefined
  function holdStart() {
    if (!ready || holding) return
    holding = true
    holdTimer = setTimeout(() => {
      holding = false
      decide('approve')
    }, HOLD)
  }
  function holdEnd() {
    clearTimeout(holdTimer)
    holding = false
  }

  onMount(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)')
    const phone = matchMedia('(max-width: 560px)')
    const updateVisibility = () => (foreground = !document.hidden)
    const updateWidth = () => (narrow = phone.matches)
    reduced = motion.matches
    updateVisibility()
    updateWidth()
    document.addEventListener('visibilitychange', updateVisibility)
    phone.addEventListener('change', updateWidth)
    const observer = new IntersectionObserver(
      ([entry]) => (visible = entry?.isIntersecting ?? false)
    )
    observer.observe(root)
    record({
      glyph: 'discuss',
      from: 'agent',
      to: 'Agent Discourse',
      note: 'Signed with its own Agent ID'
    })
    record({
      glyph: 'folder',
      from: 'agent',
      to: 'Files',
      note: 'Only the folder it was granted'
    })
    if (reduced) stillRequest()
    else void story()
    return () => {
      observer.disconnect()
      document.removeEventListener('visibilitychange', updateVisibility)
      phone.removeEventListener('change', updateWidth)
      clearTimeout(readyTimer)
      clearTimeout(holdTimer)
      timers = []
      tweens = []
    }
  })
</script>

<figure
  class="hero-art"
  class:playing
  class:waiting
  bind:this={root}
  aria-label={$t(
    'You, a teammate, your agent and a website reach your private space through the dMsg gate. Your agent also posts to the agent network. Requests that need a decision wait for you in the approval window.'
  )}
>
  <div class="hero-stage" bind:this={stage}>{@html artwork}</div>
  <div class="hero-console">
    <ol class="hero-log" aria-hidden="true">
      {#each entries as entry (entry.id)}
        <li
          class={entry.tone}
          animate:flip={{ duration: reduced ? 0 : 300 }}
          in:fly={{ y: -10, duration: reduced ? 0 : 320 }}
          ><time>{entry.time}</time><Glyph name={entry.glyph} size={15} /><span
            >{#if entry.from}<b
                ><bdi
                  >{TRANSLATED.has(entry.from)
                    ? $t(NAMES[entry.from])
                    : NAMES[entry.from]}</bdi
                ></b
              >{/if}{#if entry.to}<Glyph name="arrow-right" size={12} /><b
                >{$t(entry.to)}</b
              >{/if}<small>{$t(entry.note)}</small></span
          ></li
        >
      {/each}
    </ol>
    <section class="hero-window" aria-labelledby="{prefix}-window">
      <header
        ><span class="lights" aria-hidden="true"><i></i><i></i><i></i></span><p
          id="{prefix}-window">{$t('dMsg · Approval')}</p
        ></header
      >
      <!-- Every state shares one cell, so the window keeps its height. -->
      <div class="window-body">
        <div class="window-idle" class:shown={!request}>
          <svg viewBox="0 0 20 20" width="22" height="22" aria-hidden="true"
            ><path d="M10 3 17 10 10 17 3 10Z" /></svg
          >
          <p
            ><strong>{$t('Nothing is waiting for you.')}</strong><span
              >{$t('Approved without you: {count}', { count: 0 })}</span
            ><small
              >{$t(
                'Requests from agents, websites and the terminal open here.'
              )}</small
            ></p
          >
        </div>
        {#each REQUESTS as item}
          {@const shown = item === request && !outcome}
          <div class="window-ticket" class:shown inert={!shown}>
            <p class="ticket-head"
              ><span class="ticket-kind"
                ><i></i>{$t(KIND_LABEL[item.kind])}</span
              ><code dir="ltr">{item.via}</code>{#if !reduced}<time dir="ltr"
                  >0:{String(shown ? seconds : 15).padStart(2, '0')}</time
                >{/if}</p
            >
            <p class="ticket-title">{$t(item.title, item.params)}</p>
            <p class="ticket-effect">{$t(item.effect)}</p>
            {#if !reduced}<span class="ticket-expiry" aria-hidden="true"
                ><i style:transform="scaleX({shown ? left : 1})"></i></span
              >{/if}
            <div class="ticket-actions">
              <button class="ticket-reject" onclick={() => decide('reject')}
                >{$t('Reject')}</button
              >
              {#if item.kind === 'payment'}
                <button
                  class="ticket-approve"
                  class:ready
                  class:holding
                  aria-disabled={!ready}
                  onpointerdown={holdStart}
                  onpointerup={holdEnd}
                  onpointerleave={holdEnd}
                  onpointercancel={holdEnd}
                  onkeydown={(event) => {
                    if (
                      (event.key === 'Enter' || event.key === ' ') &&
                      !event.repeat
                    ) {
                      event.preventDefault()
                      holdStart()
                    }
                  }}
                  onkeyup={holdEnd}
                  ><i aria-hidden="true"></i><span>{$t('Hold to approve')}</span
                  ></button
                >
              {:else}
                <button
                  class="ticket-approve"
                  class:ready
                  aria-disabled={!ready}
                  onclick={() => ready && decide('approve')}
                  ><i aria-hidden="true"></i><span>{$t('Approve')}</span
                  ></button
                >
              {/if}
            </div>
          </div>
        {/each}
        <p
          class="window-outcome {outcome?.tone ?? ''}"
          class:shown={!!outcome}
          role="status"
        >
          {#if outcome}<Glyph
              name={outcome.tone === 'ok' ? 'check' : 'close'}
              size={20}
            /><span>{$t(outcome.text)}</span>{/if}
        </p>
      </div>
    </section>
  </div>
  <figcaption>
    <span class="art-caption"
      ><i aria-hidden="true"></i>{$t(
        'Interactive example · sample data · nothing is sent'
      )}</span
    >
    {#if !reduced}
      <button
        class="art-motion"
        aria-label={enabled ? $t('Pause animation') : $t('Play animation')}
        aria-pressed={!enabled}
        onclick={() => (enabled = !enabled)}
        ><Glyph name={enabled ? 'pause' : 'play'} size={16} /></button
      >
    {/if}
  </figcaption>
</figure>
