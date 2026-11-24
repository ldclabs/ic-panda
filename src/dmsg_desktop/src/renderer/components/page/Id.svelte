<script lang="ts">
  import { copyText } from '../clipboard'
  import Icon from '../Icon.svelte'
  import Tooltip from '../Tooltip.svelte'
  import { cn } from '../ui'
  import { t } from '../../i18n/index.svelte'
  import { shortId } from '../../shell/session'
  import { idChunks } from '../id-chunks'
  import { pop } from '../motion'

  /**
   * An identifier as a neutral chip: shortened unless `full`, the whole of
   * it in a tooltip, copied on a click. It reads left to right in every
   * language. `wrap` breaks a full identifier over lines instead of
   * truncating it. With `copyLabel` its copy mark turns to a check for a
   * moment after a copy, in place of a notice; the status says it for a
   * screen reader.
   */
  let { id, full = false, wrap = false, copyLabel, disabled = false, testid, class: className }: { id: string; full?: boolean; wrap?: boolean; copyLabel?: string; disabled?: boolean; testid?: string; class?: string } = $props()
  let copiedId = $state<string | null>(null)
  const copied = $derived(copiedId === id)
  let timer: ReturnType<typeof setTimeout> | undefined

  async function copy() {
    const value = id
    if (!copyLabel) return void copyText(value)
    if (!(await copyText(value, null))) return
    copiedId = value
    clearTimeout(timer)
    timer = setTimeout(() => (copiedId = null), 1200)
  }

  $effect(() => () => clearTimeout(timer))
</script>

<Tooltip content={`${id} · ${copyLabel ?? t('copy')}`}>
  {#snippet children(props)}
    <button
      {...props}
      class={cn(
        'inline-flex max-w-full min-w-0 items-center gap-2 rounded-md bg-wash px-1.5 font-mono text-code leading-5 text-ink-2 outline-none transition-colors duration-100 hover:not-disabled:bg-paper-dim hover:not-disabled:text-ink focus-visible:ring-2 focus-visible:ring-focus/35 disabled:opacity-50',
        copyLabel && 'min-h-8 py-1',
        className
      )}
      onclick={copy}
      aria-label={copyLabel}
      {disabled}
      data-testid={testid}
    >
      <bdi dir="ltr" class={cn('min-w-0 text-start', wrap ? 'break-all' : 'truncate')}>{#if full}{#each idChunks(id) as part, i}<span class={i ? 'ms-[0.4ch]' : undefined}>{part}</span>{/each}{:else}{shortId(id)}{/if}</bdi>
      {#if copyLabel}{#key copied}<span class="inline-flex shrink-0" in:pop><Icon name={copied ? 'check' : 'copy'} class={copied ? 'text-forest' : undefined} /></span>{/key}{/if}
    </button>
  {/snippet}
</Tooltip>
{#if copyLabel}<span class="sr-only" role="status">{copied ? t('copiedText') : ''}</span>{/if}
