<script lang="ts">
  import { displays } from '../display.svelte'
  import { t } from '../i18n/index.svelte'
  import { when } from '../rpc'
  let { methods }: { methods: string[] } = $props()
  const stale = $derived(methods.map((m) => displays[m]).filter((s) => s?.stale && s.observed_at_ms))
  const observed = $derived(stale.length ? Math.min(...stale.map((s) => s.observed_at_ms!)) : null)
</script>
{#if observed}
  <p class="px-5 py-2 text-xs text-muted" role="status" data-testid="display-stale">{t('cachedDisplay')} · {when(observed)}</p>
{/if}
