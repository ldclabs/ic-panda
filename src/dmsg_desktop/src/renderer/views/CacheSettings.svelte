<script lang="ts">
  import { onMount } from 'svelte'
  import { call, errorText, size } from '../rpc'
  import { t } from '../i18n/index.svelte'
  import { buttonClass } from '../components/ui'
  import Section from '../components/page/Section.svelte'
  import Notice from '../components/page/Notice.svelte'
  let bytes = $state(0)
  let error = $state('')
  let busy = $state(false)
  async function load() {
    try { const status: any = await call('cache/status'); bytes = status.domains.reduce((n: number, d: any) => n + d.sealed_bytes, 0); error = '' }
    catch (e) { error = errorText(e) }
  }
  async function clear() {
    busy = true
    error = ''
    try { await call('cache/clear'); await load() }
    catch (e) { error = errorText(e) }
    finally { busy = false }
  }
  onMount(() => { void load() })
</script>
<Section title={t('displayCache')} description={t('displayCacheHint')} icon="folder">
  <div class="flex items-center justify-between gap-3">
    <span class="text-sm text-muted">{size(bytes)}</span>
    <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={clear} data-testid="cache-clear">{t('clearDisplayCache')}</button>
  </div>
  {#if error}<Notice tone="danger">{error}</Notice>{/if}
</Section>
