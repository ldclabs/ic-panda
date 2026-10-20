<script lang="ts">
  import { copyText } from '../clipboard'
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
  let { id, full = false, wrap = false, testid, class: className }: { id: string; full?: boolean; wrap?: boolean; testid?: string; class?: string } = $props()
</script>

<Tooltip content={`${id} · ${t('copy')}`}>
  {#snippet children(props)}
    <button
      {...props}
      class={cn(
        'inline-flex max-w-full min-w-0 items-center rounded-md bg-wash px-1.5 font-mono text-[11.5px] leading-5 text-ink-2 outline-none transition-colors duration-100 hover:bg-paper-dim hover:text-ink focus-visible:ring-2 focus-visible:ring-focus/35',
        className
      )}
      onclick={() => copyText(id)}
      data-testid={testid}
    >
      <bdi dir="ltr" class={cn('min-w-0', wrap ? 'break-all' : 'truncate')}>{full ? id : shortId(id)}</bdi>
    </button>
  {/snippet}
</Tooltip>
