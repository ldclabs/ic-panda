<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { call, errorText, size } from '../rpc'
  import { t } from '../i18n/index.svelte'
  import { buttonClass } from '../components/ui'
  import Section from '../components/page/Section.svelte'
  import Notice from '../components/page/Notice.svelte'

  let alive = true
  onDestroy(() => { alive = false })
  let bytes = $state<number | null>(null)
  let error = $state('')
  let busy = $state(false)

  async function load() {
    if (!alive || busy) return
    busy = true
    try {
      const status: any = await call('cache/status')
      if (!alive) return
      bytes = status.domains.reduce((n: number, d: any) => n + d.sealed_bytes, 0)
      error = ''
    } catch (e) { if (alive) error = errorText(e) }
    finally { if (alive) busy = false }
  }

  async function clear() {
    if (busy || !bytes || error) return
    busy = true
    try {
      await call('cache/clear')
      if (!alive) return
      bytes = null
      error = ''
    } catch (e) { if (alive) error = errorText(e) }
    finally { if (alive) busy = false }
    if (alive && !error) await load()
  }

  onMount(() => { void load() })
</script>
<Section title={t('displayCache')} description={t('displayCacheHint')} icon="folder">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <span class="text-sm text-muted" role="status">{bytes === null ? error ? '-' : t('loading') : size(bytes)}</span>
    <button class={buttonClass('secondary', 'sm')} disabled={busy || !bytes || !!error} onclick={clear} data-testid="cache-clear">{t('clearDisplayCache')}</button>
  </div>
  {#if error}<Notice failure={error} tone="danger">{error}
    {#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={load}>{t('retry')}</button>{/snippet}
  </Notice>{/if}
</Section>
