<script lang="ts">
  import { directional, glyphs, type Glyph } from './glyphs'
  import { cn } from './ui'

  /** A Remix Icon; close-line uses a tighter viewBox so its cross remains legible. */
  let { name, label, class: className }: { name: Glyph; label?: string; class?: string } = $props()
</script>

<svg
  class={cn('size-4 shrink-0', directional.has(name) && 'rtl:-scale-x-100', className)}
  viewBox={name === 'close' ? '3 3 18 18' : '0 0 24 24'}
  fill="currentColor"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}><path d={glyphs[name]} /></svg>
