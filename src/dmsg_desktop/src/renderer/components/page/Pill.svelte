<script lang="ts">
  import type { Snippet } from 'svelte'
  import { still } from '../motion'
  import { cn } from '../ui'

  /**
   * A short state beside something: active, pending, revoked, paid. When its
   * tone changes in place, the colours cross over and the words rise in.
   */
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

  function rise(node: HTMLElement, key: string) {
    return {
      update(next: string) {
        if (next === key) return
        key = next
        if (!still()) node.animate([{ opacity: 0, transform: 'translateY(3px)' }, { opacity: 1, transform: 'none' }], { duration: 180, easing: 'cubic-bezier(0.22, 1, 0.36, 1)' })
      }
    }
  }
</script>

<span class={cn('inline-flex h-5 shrink-0 items-center gap-1 rounded-md px-1.5 text-caption font-medium whitespace-nowrap transition-colors duration-180 ease-calm [&_svg]:size-3', tones[tone], className)} data-testid={testid} use:rise={tone}>
  {@render children()}
</span>
