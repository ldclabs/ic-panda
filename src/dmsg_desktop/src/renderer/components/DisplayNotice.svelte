<script lang="ts">
  import { displays, displayKey } from '../display.svelte'
  import { t } from '../i18n/index.svelte'
  import { when, errorText, CallError } from '../rpc'
  let { methods, params = {} }: { methods: string[]; params?: Record<string, unknown> } = $props()
  const states = $derived(methods.map((m) => displays[displayKey(m, params)]).filter(Boolean))
  // Saved values stay on screen without comment while they refresh; age matters
  // only once a refresh failed, and that failure is said once, as their age.
  const refreshing = $derived(states.some((s) => s.refreshing && s.state === 'missing'))
  const stale = $derived(states.filter((s) => s.stale && s.error && s.observed_at_ms))
  const errors = $derived([...new Set(states.filter((s) => s.error && !stale.includes(s)).map((s) => s.error))])
  const observed = $derived(stale.length ? Math.min(...stale.map((s) => s.observed_at_ms!)) : null)
</script>
{#if observed}
  <p class="px-5 py-2 text-xs text-muted" role="status" data-testid="display-stale">{t('cachedDisplay')} · {when(observed)}</p>
{/if}

{#if refreshing}<p class="px-5 py-2 text-xs text-muted" role="status">{t('loading')}</p>{/if}
{#each errors as error}<p class="px-5 py-2 text-xs text-muted" role="status">{errorText(new CallError(`${error}: display refresh`))}</p>{/each}
