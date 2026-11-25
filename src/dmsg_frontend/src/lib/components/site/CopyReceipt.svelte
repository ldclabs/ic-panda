<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '$lib/i18n'

  let { text, describedBy }: { text: string; describedBy: string } = $props()
  let copied = $state<boolean | null>(null)
  let timer: ReturnType<typeof setTimeout>

  async function copy() {
    try {
      await navigator.clipboard.writeText(text)
      clearTimeout(timer)
      copied = true
      timer = setTimeout(() => (copied = null), 2500)
    } catch {
      clearTimeout(timer)
      copied = false
    }
  }

  onDestroy(() => clearTimeout(timer))
</script>

<button
  type="button"
  class="copy-receipt"
  aria-describedby={describedBy}
  onclick={copy}
>
  {$t('Copy receipt')}
</button>
<span role="status"
  >{copied === null ? '' : $t(copied ? 'Copied' : 'Could not copy')}</span
>
{#if copied === false}
  <textarea
    readonly
    rows="5"
    dir="auto"
    spellcheck="false"
    aria-label={$t('Copy receipt')}
    aria-describedby={describedBy}
    value={text}></textarea>
{/if}

<style>
  textarea {
    inline-size: 100%;
    min-inline-size: 0;
    padding: 8px;
    border: 1px solid var(--muted);
    border-radius: 8px;
    color: var(--ink);
    background: var(--canvas);
    font: inherit;
    resize: vertical;
  }
</style>
