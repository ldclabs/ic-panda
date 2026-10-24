<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { Glyph } from '../glyphs'
  import Icon from '../Icon.svelte'
  import { cn } from '../ui'
  import { failureTone, networkFailure } from '../../failure'
  import { network } from '../../network.svelte'

  /** A line that needs attention: something to know, a warning, an error, or a success. */
  let {
    tone = 'info',
    failure,
    icon,
    testid,
    class: className,
    action,
    children
  }: {
    tone?: 'info' | 'warn' | 'danger' | 'ok'
    failure?: string | string[]
    icon?: Glyph
    testid?: string
    class?: string
    action?: Snippet
    children: Snippet
  } = $props()

  const tones = {
    info: 'text-muted [&>svg]:text-faint',
    warn: 'border border-warn/35 bg-warn-pale text-ink-2 [&>svg]:text-warn',
    danger: 'border border-danger/30 bg-danger-pale text-danger [&>svg]:text-danger',
    ok: 'bg-ok-pale text-ink-2 [&>svg]:text-ok'
  }
  const icons: Record<string, Glyph> = { info: 'info', warn: 'warning', danger: 'warning', ok: 'check' }
  const shownTone = $derived(failure === undefined ? tone : failureTone(failure))
  const covered = $derived(network.state !== 'idle' && failure !== undefined && (Array.isArray(failure) ? failure : [failure]).filter(Boolean).every(networkFailure))
</script>

{#if !covered}
<div class={cn('flex items-start gap-2.5 rounded-lg px-3 py-2.5 text-[12.5px] leading-normal', tones[shownTone], className)} data-testid={testid} role={shownTone === 'danger' ? 'alert' : 'status'}>
  <Icon name={icon ?? icons[shownTone]} class="mt-px shrink-0" />
  <div class="min-w-0 flex-1 [overflow-wrap:anywhere]">{@render children()}</div>
  {#if action}<div class="-my-0.5 shrink-0">{@render action()}</div>{/if}
</div>
{/if}
