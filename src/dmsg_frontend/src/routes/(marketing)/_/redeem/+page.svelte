<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { t, locale } from '$lib/i18n'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import { normalizePromo, publicPromo, type PublicPromo } from '$lib/promo'
  const uid = $props.id()
  let draft = $state(''),
    copied = $state(false),
    failed = $state(false),
    copyStatus = $state(''),
    info = $state<PublicPromo>({})
  let lastAnnouncement = ''
  let copyRequest = 0
  let copyTimer: ReturnType<typeof setTimeout>
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
  onDestroy(() => {
    ++copyRequest
    clearTimeout(copyTimer)
  })
  $effect(() => {
    const value = normalized,
      controller = new AbortController()
    info = {}
    copied = false
    failed = false
    copyStatus = ''
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
    const req = ++copyRequest
    clearTimeout(copyTimer)
    let ok = false
    try {
      await navigator.clipboard.writeText(value)
      ok = true
    } catch {
      ok = false
    }
    if (req !== copyRequest || value !== normalized) return
    copied = ok
    failed = !ok
    let message = $t(ok ? 'Copied' : 'Could not copy')
    if (message === lastAnnouncement) message += '\u00A0'
    lastAnnouncement = message
    copyStatus = ''
    requestAnimationFrame(() => {
      if (req === copyRequest) copyStatus = message
    })
    if (ok) {
      copyTimer = setTimeout(() => {
        copied = false
        copyStatus = ''
      }, 2500)
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
  <div class="code-field">
    <label for={`${uid}-code`}>{$t('Membership code')}</label>
    <input
      id={`${uid}-code`}
      bind:value={draft}
      maxlength="96"
      autocomplete="off"
      autocorrect="off"
      autocapitalize="none"
      spellcheck="false"
      dir="ltr"
    />
  </div>
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
    <button
      type="button"
      class="button secondary"
      disabled={!normalized}
      onclick={copy}>{$t(copied ? 'Copied' : 'Copy')}</button
    >
    <a class="text-link" rel="noreferrer" href="/#release" data-sveltekit-reload
      >{$t('Download dMsg')}</a
    >
  </div>
  <span class="sr-only" role="status">{copyStatus}</span>
  {#if failed}<p>{$t('Membership code')} · <code>{normalized}</code></p>{/if}
</UtilityPage>

<style>
  .code-field {
    display: grid;
    gap: 0.5rem;
    max-width: 28rem;
  }
  .code-field label {
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
    font-size: max(1rem, 1em);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
    margin-top: 1rem;
  }
</style>
