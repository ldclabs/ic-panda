<script lang="ts">
  import { copyText } from '../clipboard'
  import Icon from '../Icon.svelte'
  import Tooltip from '../Tooltip.svelte'
  import { cn } from '../ui'
  import { t } from '../../i18n/index.svelte'
  import { shortId } from '../../shell/session'

  /**
   * An identifier as a neutral chip: shortened unless `full`, the whole of
   * it in a tooltip, copied on a click. It reads left to right in every
   * language. `wrap` breaks a full identifier over lines instead of
   * truncating it.
   */
  let { id, full = false, wrap = false, copyLabel, disabled = false, testid, class: className }: { id: string; full?: boolean; wrap?: boolean; copyLabel?: string; disabled?: boolean; testid?: string; class?: string } = $props()
</script>

<Tooltip content={`${id} · ${copyLabel ?? t('copy')}`}>
  {#snippet children(props)}
    <button
      {...props}
      class={cn(
        'inline-flex max-w-full min-w-0 items-center gap-2 rounded-md bg-wash px-1.5 font-mono text-[11.5px] leading-5 text-ink-2 outline-none transition-colors duration-100 hover:not-disabled:bg-paper-dim hover:not-disabled:text-ink focus-visible:ring-2 focus-visible:ring-focus/35 disabled:opacity-50',
        copyLabel && 'min-h-8 py-1',
        className
      )}
      onclick={() => copyText(id)}
      aria-label={copyLabel}
      {disabled}
      data-testid={testid}
    >
      <bdi dir="ltr" class={cn('min-w-0 text-start', wrap ? 'break-all' : 'truncate')}>{full ? id : shortId(id)}</bdi>
      {#if copyLabel}<Icon name="copy" class="shrink-0" />{/if}
    </button>
  {/snippet}
</Tooltip>
