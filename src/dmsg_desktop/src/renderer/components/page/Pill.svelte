<script lang="ts">
  import type { Snippet } from 'svelte'
  import { cn } from '../ui'

  /** A short state beside something: active, pending, revoked, paid. */
  let {
    tone = 'neutral',
    testid,
    class: className,
    children
  }: { tone?: 'neutral' | 'forest' | 'gem' | 'warn' | 'danger' | 'ok'; testid?: string; class?: string; children: Snippet } = $props()

  const tones = {
    neutral: 'bg-wash text-ink-2',
    forest: 'bg-pale text-forest',
    gem: 'bg-gem-pale text-gem',
    warn: 'bg-warn-pale text-warn',
    danger: 'bg-danger-pale text-danger',
    ok: 'bg-ok-pale text-ok'
  }
</script>

<span class={cn('inline-flex h-5 shrink-0 items-center gap-1 rounded-md px-1.5 text-[11px] font-medium whitespace-nowrap [&_svg]:size-3', tones[tone], className)} data-testid={testid}>
  {@render children()}
</span>
