<script lang="ts">
  import { onDestroy } from 'svelte'
  import { t } from '$lib/i18n'

  let { text, describedBy }: { text: string; describedBy: string } = $props()
  let copied = $state<boolean | null>(null)
  let status = $state('')
  let lastAnnouncement = ''
  let request = 0
  let timer: ReturnType<typeof setTimeout>

  async function copy() {
    const id = ++request
    clearTimeout(timer)
    let ok = false
    try {
      await navigator.clipboard.writeText(text)
      ok = true
    } catch {
      ok = false
    }
    if (id !== request) return
    copied = ok
    let message = $t(ok ? 'Copied' : 'Could not copy')
    if (message === lastAnnouncement) message += '\u00A0'
    lastAnnouncement = message
    status = ''
    requestAnimationFrame(() => {
      if (id === request) status = message
    })
    if (ok) {
      timer = setTimeout(() => {
        copied = null
        status = ''
      }, 2500)
    }
  }

  onDestroy(() => {
    ++request
    clearTimeout(timer)
  })
</script>

<button
  type="button"
  class="copy-receipt"
  aria-describedby={describedBy}
  onclick={copy}
>
  {$t('Copy receipt')}
</button>
<span role="status">{status}</span>
{#if copied === false}
  <textarea
    readonly
    rows="5"
    dir="auto"
    spellcheck="false"
    aria-label={$t('Copy receipt')}
    aria-describedby={describedBy}
    onfocus={(event) => event.currentTarget.select()}
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
    font-size: max(1rem, 1em);
    resize: vertical;
  }
</style>
