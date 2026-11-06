<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'

  /**
   * Drags the width of the pane just before it, kept for next time; arrows,
   * Home and End do the same, and a double click goes back to the default. The
   * pane takes the width as `--pane`, which falls back to its own default.
   */
  let { name, min = 200, max = 520, class: className = '' }: { name: string; min?: number; max?: number; class?: string } = $props()

  const key = $derived(`dmsg.pane.${name}`)
  let handle = $state<HTMLElement | null>(null)
  let width = $state<number | null>(null)
  let from: { x: number; width: number } | null = null

  const pane = () => handle?.previousElementSibling as HTMLElement | null | undefined
  const rtl = () => document.documentElement.dir === 'rtl'

  function keep(value: string | null) {
    try {
      if (value === null) localStorage.removeItem(key)
      else localStorage.setItem(key, value)
    } catch {
      /* the width lasts until the window closes */
    }
  }

  function set(next: number) {
    width = Math.round(Math.max(min, Math.min(max, next)))
    pane()?.style.setProperty('--pane', `${width}px`)
  }

  function onPointerDown(event: PointerEvent) {
    const shown = pane()?.getBoundingClientRect().width
    if (!shown || event.button !== 0) return
    event.preventDefault()
    handle?.setPointerCapture(event.pointerId)
    from = { x: event.clientX, width: shown }
  }

  function onPointerMove(event: PointerEvent) {
    if (!from) return
    const moved = event.clientX - from.x
    set(from.width + (rtl() ? -moved : moved))
  }

  function onPointerUp() {
    if (from && width !== null) keep(String(width))
    from = null
  }

  function onKeyDown(event: KeyboardEvent) {
    const shown = width ?? pane()?.getBoundingClientRect().width
    if (!shown) return
    const wider = rtl() ? 'ArrowLeft' : 'ArrowRight'
    const narrower = rtl() ? 'ArrowRight' : 'ArrowLeft'
    if (event.key === wider) set(shown + 16)
    else if (event.key === narrower) set(shown - 16)
    else if (event.key === 'Home') set(min)
    else if (event.key === 'End') set(max)
    else return
    event.preventDefault()
    keep(String(width))
  }

  function reset() {
    width = null
    pane()?.style.removeProperty('--pane')
    keep(null)
  }

  onMount(() => {
    let kept: number | null = null
    try {
      kept = Number(localStorage.getItem(key)) || null
    } catch {
      /* none kept */
    }
    if (kept) set(kept)
  })
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  bind:this={handle}
  role="separator"
  aria-orientation="vertical"
  aria-label={t('paneWidth')}
  aria-valuemin={min}
  aria-valuemax={max}
  aria-valuenow={width ?? undefined}
  tabindex="0"
  class="relative z-10 -mx-1 w-2 shrink-0 cursor-col-resize touch-none outline-none transition-colors duration-100 hover:bg-forest/15 focus-visible:bg-forest/25 active:bg-forest/25 {className}"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onkeydown={onKeyDown}
  ondblclick={reset}
  data-testid={`pane-handle-${name}`}
></div>
