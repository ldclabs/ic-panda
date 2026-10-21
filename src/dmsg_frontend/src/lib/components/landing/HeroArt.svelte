<script lang="ts">
  import { onMount } from 'svelte'
  import { locale, t } from '$lib/i18n'
  import Glyph from './Glyph.svelte'
  import { glyphs } from './glyphs'
  import { renderHeroArt } from './hero-art.js'

  const id = $props.id()
  let root: HTMLElement
  let enabled = $state(true)
  let visible = $state(false)
  let reduced = $state(true)
  let foreground = $state(true)
  const playing = $derived(enabled && visible && foreground && !reduced)
  const artwork = $derived(
    renderHeroArt({
      prefix: `hero-${id}`,
      labels: {
        space: $t('Private space'),
        network: $t('Agent network'),
        teammate: $t('Teammate'),
        agent: $t('Your agent'),
        owner: $t('Owner'),
        you: $t('You'),
        scope: $t('Scoped access')
      },
      icons: glyphs,
      rtl: $locale === 'ar'
    })
  )

  onMount(() => {
    const motion = matchMedia('(prefers-reduced-motion: reduce)')
    const updateMotion = () => (reduced = motion.matches)
    const updateVisibility = () => (foreground = !document.hidden)
    updateMotion()
    updateVisibility()
    motion.addEventListener('change', updateMotion)
    document.addEventListener('visibilitychange', updateVisibility)
    const observer = new IntersectionObserver(
      ([entry]) => (visible = entry?.isIntersecting ?? false)
    )
    observer.observe(root)
    return () => {
      observer.disconnect()
      motion.removeEventListener('change', updateMotion)
      document.removeEventListener('visibilitychange', updateVisibility)
    }
  })
</script>

<figure
  class="hero-art"
  class:playing
  bind:this={root}
  aria-label={$t(
    'You, a teammate and your agent connect to a private workspace. The agent also connects to Discourse, Mail and Knowledge.'
  )}
>
  <div class="hero-art-scene">{@html artwork}</div>
  <figcaption>
    <span class="art-caption"
      ><i aria-hidden="true"></i>{$t(
        'A private space. A connected world.'
      )}</span
    >
    {#if !reduced}
      <button
        class="art-motion"
        aria-label={enabled ? $t('Pause animation') : $t('Play animation')}
        aria-pressed={enabled}
        onclick={() => (enabled = !enabled)}
        ><Glyph name={enabled ? 'pause' : 'play'} size={16} /></button
      >
    {/if}
  </figcaption>
</figure>
