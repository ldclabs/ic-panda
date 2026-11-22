<script lang="ts">
  import { onMount } from 'svelte'
  import { t, locale } from '$lib/i18n'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import { normalizePromo, publicPromo, type PublicPromo } from '$lib/promo'
  let draft = $state(''),
    copied = $state(false),
    failed = $state(false),
    info = $state<PublicPromo>({})
  let normalized = $derived(normalizePromo(draft))
  onMount(() => {
    const read = () => {
      draft = location.hash.slice(1, 97)
      copied = false
    }
    read()
    window.addEventListener('hashchange', read)
    return () => window.removeEventListener('hashchange', read)
  })
  $effect(() => {
    const value = normalized,
      controller = new AbortController()
    info = {}
    copied = false
    failed = false
    const timer = setTimeout(() => {
      void publicPromo(value, controller.signal)
        .then((r) => {
          if (!controller.signal.aborted) info = r
        })
        .catch(() => {})
    }, 300)
    return () => {
      clearTimeout(timer)
      controller.abort()
    }
  })
  async function copy() {
    const value = normalized
    try {
      await navigator.clipboard.writeText(value)
      if (value === normalized) copied = true
    } catch {
      if (value === normalized) failed = true
    }
  }
</script>

<svelte:head
  ><title>{$t('Redeem code')}</title><meta
    name="referrer"
    content="no-referrer"
  /></svelte:head
>
<UtilityPage>
  <h1>{$t('Redeem code')}</h1>
  <p class="utility-lead"
    >{$t('Review and confirm in dMsg Desktop or the dmsg CLI.')}</p
  >
  <label class="code-field"
    >{$t('Membership code')}<input
      bind:value={draft}
      maxlength="96"
      autocomplete="off"
      spellcheck="false"
      dir="ltr"
    /></label
  >
  {#if info.valid && info.plan && info.duration_days}<p
      >{info.title} · {info.plan} · {new Intl.NumberFormat($locale, {
        style: 'unit',
        unit: 'day',
        unitDisplay: 'long'
      }).format(info.duration_days)}{#if info.ends_at_ms}
        · {$t('Expires')}
        <time datetime={new Date(info.ends_at_ms).toISOString()}
          >{new Intl.DateTimeFormat($locale, {
            dateStyle: 'medium',
            timeStyle: 'short'
          }).format(info.ends_at_ms)}</time
        >{/if}</p
    >{/if}
  {#if info.minimum_versions}<p>{info.minimum_versions}</p>{/if}
  {#if info.valid === false && normalized}<p role="status"
      >{$t('Membership code')} · {$t('Not available')}</p
    >{/if}
  <p class="utility-note">{$t('No automatic charge at expiry.')}</p>
  <div class="actions">
    {#if normalized}<a
        class="button primary"
        rel="noreferrer"
        href={'dmsg://redeem#' + normalized}>{$t('Open dMsg Desktop')}</a
      >{/if}
    <button class="button secondary" disabled={!normalized} onclick={copy}
      >{$t(copied ? 'Copied' : 'Copy')}</button
    >
    <a class="text-link" rel="noreferrer" href="/#release" data-sveltekit-reload
      >{$t('Download dMsg')}</a
    >
  </div>
  {#if failed}<p role="status"
      >{$t('Membership code')} · <code>{normalized}</code></p
    >{/if}
</UtilityPage>

<style>
  .code-field {
    display: grid;
    gap: 0.5rem;
    max-width: 28rem;
    font-weight: 600;
  }
  .code-field input {
    width: 100%;
    padding: 0.65rem 0.8rem;
    color: var(--ink);
    background: var(--paper, #fff);
    border: 1px solid var(--line);
    border-radius: 0.5rem;
    font: inherit;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-top: 1rem;
  }
</style>
