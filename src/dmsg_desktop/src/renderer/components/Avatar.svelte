<script lang="ts">
  import { initials, tone } from '../views/channels/channels'
  import { cn } from './ui'

  /** A rounded tile with a name's first letter or an ID's first two, in a tone that stays with the ID. */
  let { id, name = id, size = 'md', class: className }: { id: string; name?: string; size?: 'sm' | 'md' | 'lg'; class?: string } = $props()

  const tones = ['bg-pale text-forest', 'bg-gem-pale text-gem', 'bg-warn-pale text-warn', 'bg-wash text-ink-2']
  const sizes = { sm: 'size-6 rounded-md text-[10.5px]', md: 'size-8 rounded-lg text-[12px]', lg: 'size-10 rounded-xl text-[14px]' }
  const text = $derived(initials(name))
</script>

<span
  class={cn('grid shrink-0 place-items-center font-semibold select-none', /^[0-9a-z]{2}$/.test(text) && 'font-mono', tones[tone(id)], sizes[size], className)}
  aria-hidden="true"
>
  {text}
</span>
