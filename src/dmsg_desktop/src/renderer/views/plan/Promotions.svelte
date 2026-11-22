<script lang="ts">
  import { onDestroy } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import TextField from '../../components/TextField.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { locale, t } from '../../i18n/index.svelte'
  import { call, errorText, when } from '../../rpc'

  let {
    account,
    guest,
    redemption = $bindable(null),
    refresh
  }: {
    account: string
    guest: boolean
    redemption?: { code: string } | null
    refresh: () => Promise<void>
  } = $props()
  type Preview = {
    account: string
    eligible: boolean
    title?: string
    plan?: string
    duration_days?: number
    expires_at_ms?: number
    reason?: string
    minimum_versions?: string
  }
  type Claim = {
    grant_id: string
    status: string
    plan: string
    expires_at_ms: number
    synced: boolean
    reason?: string
  }
  type Pass = {
    code: string | null
    slot: number
    ends_at_ms?: number
  }
  let open = $state(false),
    mode = $state<'redeem' | 'guest'>('redeem'),
    draft = $state(''),
    busy = $state(false),
    failure = $state('')
  let preview = $state<Preview | null>(null),
    claim = $state<Claim | null>(null),
    pass = $state<Pass | null>(null)
  let history = $state<Claim[]>([]),
    slots = $state<{ slot: number; available: boolean }[]>([]),
    remaining = $state<number | null>(null)
  let epoch = 0,
    historyRead = 0,
    operation = '',
    replacement: number | null = null
  const nonce = () =>
    Array.from(crypto.getRandomValues(new Uint8Array(32)), (v) =>
      v.toString(16).padStart(2, '0')
    ).join('')
  const changed = () => {
    epoch++
    preview = null
    claim = null
    failure = ''
    busy = false
    operation = nonce()
  }
  function start(kind: 'redeem' | 'guest', code = '') {
    changed()
    mode = kind
    draft = code
    pass = null
    history = []
    slots = []
    remaining = null
    replacement = null
    open = true
    void loadHistory()
  }
  function close() {
    epoch++
    historyRead++
    open = false
    draft = ''
    claim = null
    pass = null
    preview = null
    failure = ''
    busy = false
  }
  onDestroy(() => {
    epoch++
    historyRead++
  })
  $effect(() => {
    if (redemption) {
      const code = redemption.code
      redemption = null
      start('redeem', code)
    }
  })
  async function loadHistory(report = mode === 'guest') {
    const read = ++historyRead
    try {
      const r = await call('plan/promo_history', { account })
      if (read === historyRead) {
        history = r.redemptions
        slots = r.guest_passes
        remaining = r.guest_remaining
      }
    } catch (error) {
      if (read === historyRead && report) failure = errorText(error)
    }
  }
  async function run(
    action: 'preview' | 'redeem' | 'guest',
    slot: number | null = null
  ) {
    if (busy) return
    const version = epoch
    busy = true
    failure = ''
    try {
      if (action === 'preview') {
        const r = await call('plan/promo_preview', { account, code: draft.trim() })
        if (version === epoch) preview = r
      } else if (action === 'redeem') {
        if (!preview?.eligible || preview.account !== account) return
        const r = await call('plan/redeem', {
          code: draft.trim(),
          account,
          request_id: operation
        })
        if (version !== epoch) return
        if (r.status === 'failed' || r.status === 'rejected') {
          changed()
          failure = [t('promoUnavailable'), r.reason].filter(Boolean).join(' ')
          return
        }
        claim = r
        if (r.status === 'applied') await refresh()
        if (version === epoch) await loadHistory()
      } else {
        if (replacement !== slot) {
          operation = nonce()
          replacement = slot
        }
        const r = await call('plan/guest_pass', {
          account,
          replace_slot: slot,
          request_id: operation
        })
        if (version === epoch) {
          pass = r
          operation = nonce()
          replacement = null
          await loadHistory()
        }
      }
    } catch (error) {
      if (version === epoch) failure = errorText(error)
    } finally {
      if (version === epoch) busy = false
    }
  }
  async function updateClaim() {
    if (busy) return
    const version = epoch
    busy = true
    failure = ''
    try {
      await loadHistory(true)
      if (version !== epoch || !claim) return
      const updated = history.find((r) => r.grant_id === claim?.grant_id)
      if (updated) {
        claim = updated
        if (updated.status === 'applied') await refresh()
      }
    } catch (error) {
      if (version === epoch) failure = errorText(error)
    } finally {
      if (version === epoch) busy = false
    }
  }
</script>

<button
  class={buttonClass('secondary', 'sm')}
  onclick={() => start('redeem')}
  data-testid="promo-open">{t('promoRedeem')}</button
>
{#if guest}<button
    class={buttonClass('secondary', 'sm')}
    onclick={() => start('guest')}
    data-testid="guest-open">{t('promoGuest')}</button
  >{/if}
<Dialog
  bind:open={
    () => open,
    (next) => {
      if (!next) close()
    }
  }
  title={t(mode === 'guest' ? 'promoGuest' : 'promoRedeem')}
  dismissible={mode !== 'guest' || !busy}
  testid="promo-dialog"
>
  <div class="grid min-w-0 gap-3">
    <p class="text-sm break-all">{t('account')} · <bdi dir="ltr">{account}</bdi></p>
    {#if mode === 'redeem'}
      <TextField
        label={t('promoCode')}
        mono
        dir="ltr"
        bind:value={
          () => draft,
          (value) => {
            draft = value
            changed()
          }
        }
        disabled={(busy && !!preview?.eligible) || !!claim}
        maxlength={96}
        autocomplete="off"
        data-testid="promo-input"
      />
      {#if preview?.eligible}<p
          ><bdi>{preview.plan}</bdi> · {new Intl.NumberFormat(locale(), {
            style: 'unit',
            unit: 'day',
            unitDisplay: 'long'
          }).format(preview.duration_days!)} · {t('expires')}
          {when(preview.expires_at_ms!)}<br />{t('promoNoCharge')}</p
        >
        {#if preview.minimum_versions}<p class="text-muted text-sm"
            >{preview.minimum_versions}</p
          >{/if}
      {:else if preview}<Notice tone="warn"
          >{t('promoUnavailable')} {preview.reason}</Notice
        >{/if}
      {#if claim}<p role="status"
          >{claim.status === 'applied'
            ? t('promoApplied')
            : claim.status === 'pending'
              ? t('promoPending')
              : t('promoUnavailable')}
          {claim.reason ?? ''}
          {claim.status === 'applied' && !claim.synced ? t('promoSync') : ''}</p
        >{/if}
      {#if history.length}<details
          ><summary>{t('history')}</summary
          >{#each history as row (row.grant_id)}<p class="break-words"
              >{row.plan} · {when(row.expires_at_ms)} · {row.status}</p
            >{/each}</details
        >{/if}
    {:else}
      {#if remaining !== null}<p>{t('remaining')} · {remaining}</p>{/if}
      {#if pass?.code}<TextField
          label={t('promoCode')}
          value={pass.code}
          readonly
          mono
          dir="ltr"
        /><button
          class={buttonClass('secondary', 'sm')}
          onclick={() => copyText(pass!.code!)}>{t('copyCode')}</button
        >{:else if pass}<Notice tone="warn" testid="guest-code-unavailable"
          >{t('promoGuestReplay', { slot: pass.slot })}</Notice
        >{/if}
      {#if pass?.ends_at_ms}<p>{t('expires')} {when(pass.ends_at_ms)}</p>{/if}
      {#each slots.filter((s) => s.available) as item (item.slot)}<button
          class={buttonClass('secondary', 'sm')}
          disabled={busy}
          onclick={() => run('guest', item.slot)}
          >{t('promoReplace')} <bdi dir="ltr">#{item.slot}</bdi></button
        >{/each}
    {/if}
    {#if failure}<Notice tone="danger" {failure}>{failure}</Notice>{/if}
  </div>
  {#snippet footer()}
    <button
      class={buttonClass('secondary')}
      disabled={mode === 'guest' && busy}
      onclick={close}
      >{t('close')}</button
    >
    {#if mode === 'guest'}<button
        class={buttonClass('primary')}
        disabled={busy || remaining === 0 || !!pass}
        onclick={() => run('guest')}>{t(busy ? 'loading' : 'promoGuest')}</button
      >
    {:else if claim}<button
        class={buttonClass('secondary')}
        disabled={busy}
        onclick={updateClaim}>{busy ? t('loading') : t('refresh')}</button
      >
    {:else}<button
        class={buttonClass('primary')}
        disabled={busy || !draft.trim() || preview?.eligible === false}
        data-testid="promo-submit"
        onclick={() => run(preview?.eligible ? 'redeem' : 'preview')}
        >{busy
          ? t('loading')
          : t(preview?.eligible ? 'promoRedeem' : 'promoCheck')}</button
      >{/if}
  {/snippet}
</Dialog>
