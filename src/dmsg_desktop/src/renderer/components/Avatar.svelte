<script lang="ts">
  import { initials, tone } from '../views/channels/channels'
  import { cn } from './ui'

  /** A rounded tile with a name's first letter or an ID's first two, in a tone that stays with the ID. */
  let { id, name = id, src, size = 'md', class: className }: { id: string; name?: string; src?: string | null; size?: 'sm' | 'md' | 'lg'; class?: string } = $props()
  let failed = $state<string | null>(null)

  // The iOS avatar tints: Gem and Warn mean a state, so a person never wears them.
  const tones = ['bg-avatar-forest text-avatar-forest-ink', 'bg-avatar-sage text-avatar-sage-ink', 'bg-avatar-amber text-avatar-amber-ink']
  const sizes = { sm: 'size-6 rounded-md text-caption', md: 'size-8 rounded-lg text-callout', lg: 'size-10 rounded-xl text-body' }
  const text = $derived(initials(name))
</script>

<span
  class={cn('grid shrink-0 place-items-center overflow-hidden font-semibold select-none', /^[0-9a-z]{2}$/.test(text) && 'font-mono', tones[tone(id, tones.length)], sizes[size], className)}
  aria-hidden="true"
>
  {#if src && failed !== src}
    <img {src} alt="" class="size-full object-cover" onerror={() => (failed = src ?? null)} />
  {:else}{text}{/if}
</span>
