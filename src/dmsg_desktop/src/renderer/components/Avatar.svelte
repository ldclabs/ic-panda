<script lang="ts">
  import { initials, tone } from '../views/channels/channels'
  import { cn } from './ui'

  /**
   * A name's first letter or an ID's first two, in a tone that stays with the
   * ID: a circle for a person, a rounded tile for a channel or an object.
   * A picture covers the letters once it has loaded, fading in unless it was
   * already there.
   */
  let { id, name = id, src, size = 'md', person = false, class: className }: { id: string; name?: string; src?: string | null; size?: 'sm' | 'md' | 'lg'; person?: boolean; class?: string } = $props()
  let failed = $state<string | null>(null)

  // The iOS avatar tints: Gem and Warn mean a state, so a person never wears them.
  const tones = ['bg-avatar-forest text-avatar-forest-ink', 'bg-avatar-sage text-avatar-sage-ink', 'bg-avatar-amber text-avatar-amber-ink']
  const sizes = { sm: 'size-6 rounded-md text-caption', md: 'size-8 rounded-lg text-callout', lg: 'size-10 rounded-xl text-body' }
  const text = $derived(initials(name))

  function loaded(img: HTMLImageElement) {
    const show = () => (img.dataset.loaded = '')
    if (img.complete && img.naturalWidth) show()
    else img.addEventListener('load', show, { once: true })
  }
</script>

<span
  class={cn('relative grid shrink-0 place-items-center overflow-hidden font-semibold select-none', /^[0-9a-z]{2}$/.test(text) && 'font-mono', tones[tone(id, tones.length)], sizes[size], person && 'rounded-full', className)}
  aria-hidden="true"
>
  {#if src && failed !== src}
    {#key src}
      <img {src} alt="" class="peer absolute inset-0 size-full object-cover opacity-0 transition-opacity duration-160 ease-calm data-loaded:opacity-100" use:loaded onerror={() => (failed = src ?? null)} />
    {/key}
  {/if}
  <span class="transition-[visibility] duration-160 peer-data-loaded:invisible">{text}</span>
</span>
