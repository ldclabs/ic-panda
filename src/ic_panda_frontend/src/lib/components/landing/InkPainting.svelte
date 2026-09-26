<script lang="ts">
  import { t } from '$lib/i18n'
  import { createMusic, type Music } from '$lib/ink/music'
  import { createInkPainting, type InkPainting } from '$lib/ink/painting'
  import { onMount } from 'svelte'

  interface Props {
    /** The hero's sheet of tracing paper; the painting leaves room for it. */
    sheet: HTMLElement | undefined
    /** The seed of the painting on the wall, for its caption. */
    seed?: number | null
  }

  let { sheet, seed = $bindable(null) }: Props = $props()

  const MUSIC_KEY = 'icpanda:music'
  /* Pointer events on these never reach the panda. */
  const UI =
    'a,button,input,select,textarea,label,summary,[role="button"],[role="dialog"],.sheet,#shell-header'

  // 题款: the year in the sexagenary cycle and the season, then where it
  // was painted, written in the painting whatever the page's language.
  const lines = (() => {
    const d = new Date()
    const m = d.getMonth()
    const y = d.getFullYear() - (m === 0 ? 1 : 0) - 4
    const season = '冬冬春春春夏夏夏秋秋秋冬'[m]
    return [
      '甲乙丙丁戊己庚辛壬癸'[y % 10]! +
        '子丑寅卯辰巳午未申酉戌亥'[y % 12]! +
        season,
      '画于链上'
    ].map((line, i, all) => {
      const before = all
        .slice(0, i)
        .reduce((n, l) => n + l.length * 130 + 200, 0)
      return Array.from(line, (ch, j) => ({ ch, delay: before + j * 130 }))
    })
  })()

  let canvas: HTMLCanvasElement
  let ghost: HTMLDivElement
  let painting: InkPainting | null = null
  let music: Music | null = null
  let failed = $state(false)
  let colophon = $state(false)
  let inscribed = $state(false)
  let sealed = $state(false)
  let colophonTop = $state(0)
  let colophonRight = $state(12)
  let canPlay = $state(false)
  let playing = $state(false)

  export function repaint() {
    painting?.repaint()
  }

  function remember(on: boolean) {
    try {
      localStorage.setItem(MUSIC_KEY, on ? 'on' : 'off')
    } catch {}
  }

  function remembered(): boolean {
    try {
      return localStorage.getItem(MUSIC_KEY) === 'on'
    } catch {
      return false
    }
  }

  async function play() {
    music ??= createMusic(seed ?? Date.now())
    if (!music) return
    playing = true
    try {
      await music.play()
    } catch (err) {
      console.warn('Music unavailable:', err)
      playing = false
    }
  }

  function toggleMusic() {
    if (playing) {
      music?.pause()
      playing = false
      remember(false)
    } else {
      remember(true)
      void play()
    }
  }

  function showColophon() {
    const wide = innerWidth >= 900 && innerWidth / innerHeight > 1.05
    const bar =
      document.getElementById('shell-header')?.getBoundingClientRect().bottom ??
      0
    colophonTop = bar + (wide ? 22 : 14)
    colophonRight = wide ? 30 : 12
    colophon = true
    requestAnimationFrame(() => requestAnimationFrame(() => (inscribed = true)))
  }

  onMount(() => {
    const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
    painting = createInkPainting(
      canvas,
      ghost,
      {
        measure() {
          const scrollTop = document.getElementById('page')?.scrollTop ?? 0
          const r = sheet?.getBoundingClientRect()
          const bar =
            document.getElementById('shell-header')?.getBoundingClientRect()
              .bottom ?? 0
          return {
            sheet: {
              top: (r?.top ?? innerHeight) + scrollTop,
              right: r?.right ?? 0
            },
            barBottom: bar
          }
        },
        isUI: (el) => el instanceof Element && !!el.closest(UI),
        onSeed: (s) => (seed = s),
        onCue(cue) {
          if (cue === 'colophon') showColophon()
          else if (cue === 'seal') {
            sealed = true
            music?.accent('seal')
          } else {
            colophon = false
            inscribed = false
            sealed = false
          }
        },
        onPoke: () => music?.accent('poke')
      },
      reduced,
      450
    )
    failed = !painting

    // Sound needs a gesture. Whoever left it on last time gets it back with
    // their first click or key press anywhere on the page.
    canPlay = typeof AudioContext !== 'undefined'
    const resume = (e: Event) => {
      if (e.target instanceof Element && e.target.closest('.ink-sound')) return
      disarm()
      if (!playing) void play()
    }
    const disarm = () => {
      removeEventListener('click', resume, true)
      removeEventListener('keydown', resume, true)
    }
    if (canPlay && remembered()) {
      addEventListener('click', resume, true)
      addEventListener('keydown', resume, true)
    }

    return () => {
      disarm()
      painting?.destroy()
      music?.destroy()
    }
  })

  // Once the hero's sheet has scrolled away, the painting is mostly under
  // other sheets and can run at a lower frame rate.
  $effect(() => {
    const el = sheet
    if (!el) return
    const io = new IntersectionObserver((entries) =>
      painting?.setQuiet(!entries[0]?.isIntersecting)
    )
    io.observe(el)
    return () => io.disconnect()
  })
</script>

<canvas class="ink" class:off={failed} bind:this={canvas} aria-hidden="true"
></canvas>
<div class="ghost" bind:this={ghost} aria-hidden="true"></div>

{#if colophon}
  <div
    class="colophon"
    class:on={inscribed}
    style:top="{colophonTop}px"
    style:right="{colophonRight}px"
    aria-hidden="true"
  >
    <div class="ins">
      {#each lines as line}
        <div>
          {#each line as c}
            <span class="ch" style:transition-delay="{c.delay}ms">{c.ch}</span>
          {/each}
        </div>
      {/each}
    </div>
    <div class="seal" class:on={sealed}>ICPanda</div>
  </div>
{/if}

{#if canPlay}
  <button
    type="button"
    class="ink-sound"
    class:on={playing}
    onclick={toggleMusic}
    aria-pressed={playing}
    aria-label={playing ? $t('Pause music') : $t('Play music')}
    title={playing ? $t('Pause music') : $t('Play music')}
  >
    <span class="bars" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
    <span class="label">{$t('Music')}</span>
  </button>
{/if}

<style>
  .ink {
    position: fixed;
    inset: 0;
    z-index: -1;
    display: block;
    width: 100%;
    height: 100%;
    background: #ece9e1;
  }

  /* No WebGL2: the page keeps its plain ground. */
  .ink.off {
    display: none;
  }

  /* The invisible hand: a faint ring that follows the brush. */
  .ghost {
    position: fixed;
    left: 0;
    top: 0;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border: 1px solid #17181b;
    border-radius: 50%;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.4s;
  }

  .ghost:global(.on) {
    opacity: 0.4;
  }

  /* 题款 + 印: an inscription written in the painting, not on the film. */
  .colophon {
    position: fixed;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    pointer-events: none;
    user-select: none;
  }

  .ins {
    writing-mode: vertical-rl;
    font-family:
      'Kaiti SC', 'STKaiti', 'KaiTi', 'BiauKai', 'AR PL UKai CN', serif;
    font-size: clamp(17px, 2.3vmin, 24px);
    line-height: 1.55;
    letter-spacing: 0.2em;
    color: #17181b;
  }

  .ch {
    display: inline-block;
    opacity: 0;
    filter: blur(5px);
    transition:
      opacity 1s ease,
      filter 1.2s ease;
  }

  .on .ch {
    opacity: 0.86;
    filter: none;
  }

  /* A long seal cut in relief: pale letters and an inner frame on cinnabar. */
  .seal {
    padding: 6px 8px 5px;
    background: #b23b2b;
    box-shadow:
      inset 0 0 0 2px #b23b2b,
      inset 0 0 0 3px rgb(243 239 230 / 85%);
    color: #f3efe6;
    font:
      600 12px/1 'IBM Plex Mono',
      ui-monospace,
      monospace;
    letter-spacing: 0.04em;
    white-space: nowrap;
    border-radius: 2px 3px 2px 4px;
    mix-blend-mode: multiply;
    opacity: 0;
    transform: scale(1.6) rotate(-8deg);
    transition:
      opacity 0.18s ease-in,
      transform 0.22s cubic-bezier(0.3, 1.6, 0.5, 1);
  }

  .seal.on {
    opacity: 0.88;
    transform: rotate(-2deg);
  }

  .ink-sound {
    position: fixed;
    right: max(24px, env(safe-area-inset-right));
    bottom: max(20px, env(safe-area-inset-bottom));
    z-index: 20;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 16px 0 14px;
    border: 1px solid var(--film-edge);
    border-radius: 999px;
    background: var(--film-bar);
    backdrop-filter: blur(8px);
    color: var(--ink);
    font:
      500 11px/1 'IBM Plex Mono',
      ui-monospace,
      SFMono-Regular,
      Menlo,
      Consolas,
      monospace;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    transition:
      background-color 0.2s,
      color 0.2s;
  }

  .ink-sound:hover {
    background: var(--ink);
    color: var(--paper);
  }

  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }

  .bars i {
    width: 2px;
    height: 100%;
    background: currentColor;
    transform: scaleY(var(--h));
    transform-origin: bottom;
    transition: transform 0.4s;
  }

  .bars i:nth-child(1) {
    --h: 0.4;
  }

  .bars i:nth-child(2) {
    --h: 0.85;
  }

  .bars i:nth-child(3) {
    --h: 0.55;
  }

  .bars i:nth-child(4) {
    --h: 0.7;
  }

  .on .bars i {
    animation: bar 1.6s ease-in-out infinite alternate;
  }

  .on .bars i:nth-child(2) {
    animation-duration: 2.3s;
    animation-delay: -0.8s;
  }

  .on .bars i:nth-child(3) {
    animation-duration: 1.9s;
    animation-delay: -1.3s;
  }

  .on .bars i:nth-child(4) {
    animation-duration: 2.7s;
    animation-delay: -0.4s;
  }

  @keyframes bar {
    from {
      transform: scaleY(0.2);
    }

    to {
      transform: scaleY(1);
    }
  }

  @media (max-width: 639px) {
    .ink-sound {
      padding: 0 13px;
    }

    .label {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .ch,
    .seal {
      transition-duration: 0.01s;
    }

    .on .bars i {
      animation: none;
    }
  }
</style>
