<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Select from '../components/Select.svelte'
  import Checkbox from '../components/Checkbox.svelte'
  import { buttonClass } from '../components/ui'

  let { status }: { status: any } = $props()
  let letters = $state<any[]>([])
  let outgoing = $state<any[]>([])
  let policy = $state<any>(null)
  let mode = $state('contacts')
  let price = $state('')
  let recipient = $state('')
  let text = $state('')
  let pay = $state(false)
  let sent = $state<any>(null)
  let escrows = $state<Record<string, any>>({})
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  const load = () =>
    attempt(async () => {
      letters = await call('inbox/list')
      outgoing = await call('inbox/outgoing')
      policy = await call('inbox/policy')
      if (policy) mode = policy.mode
    })

  const configure = () =>
    attempt(async () => {
      policy = await call('inbox/configure', mode === 'paid' ? { mode, price: price.trim() } : { mode })
    })

  /** A free letter is delivered at once; a paid one comes back quoted and is paid only on approval. */
  const send = () =>
    attempt(async () => {
      const to = recipient.trim()
      sent = await call('inbox/send', { recipient: to, text, pay })
      if (sent.state === 'quoted') {
        const request = await call('inbox/pay', { recipient: to, order: sent.order_id })
        await window.dmsg.openApproval(request.id)
      }
      text = ''
      outgoing = await call('inbox/outgoing')
    })

  const mark = (letter: any, archived: boolean) =>
    attempt(async () => {
      await call('inbox/mark', { order: letter.order_id, read: true, archived })
      letters = await call('inbox/list')
    })

  const check = (order: any) =>
    attempt(async () => {
      sent = await call('inbox/order', { recipient: order.recipient, order: order.order_id })
    })

  /** The escrow of a paid letter and its transfers; a refused one is resent at the fee the ledger asked for. */
  const funds = (order: any) =>
    attempt(async () => {
      escrows[order.order_id] = await call('inbox/escrow', { order: order.order_id })
    })

  const revise = (order: any, leg: any) =>
    attempt(async () => {
      await call('payment/revise', { escrow: leg.escrow_id, leg: leg.leg_id })
      escrows[order.order_id] = await call('inbox/escrow', { order: order.order_id })
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['inbox/changed', 'session/changed', 'approval/changed'].includes(event.type)) void load()
    })
  })

  const unlocked = $derived(status?.state === 'unlocked')
  const shown = $derived(letters.filter((l) => !l.archived))
</script>

{#if error}<p class="error" data-testid="inbox-error">{error}</p>{/if}
<ul class="list card" data-testid="letters">
  {#each shown as letter (letter.order_id)}
    <li>
      <div class="row spread">
        <span class="mono muted">{letter.sender}</span>
        <span class="muted">{when(letter.created_at)}</span>
      </div>
      {#if letter.state === 'visible'}
        <div class:unread={!letter.read}>{letter.text}</div>
      {:else}
        <div class="muted">{t('aborted')}: {letter.abort_reason ?? letter.state}</div>
      {/if}
      <div class="row">
        {#if !letter.read}<button class={buttonClass()} disabled={busy} onclick={() => mark(letter, false)}>{t('markRead')}</button>{/if}
        <button class={buttonClass()} disabled={busy} onclick={() => mark(letter, true)}>{t('archive')}</button>
      </div>
    </li>
  {:else}
    <li class="muted">{t('noLetters')}</li>
  {/each}
</ul>
{#if unlocked}
  <div class="card">
    <h2>{t('writeLetter')}</h2>
    <input placeholder={t('accountId')} bind:value={recipient} data-testid="letter-recipient" />
    <textarea placeholder={t('message')} bind:value={text} data-testid="letter-text"></textarea>
    <Checkbox bind:checked={pay} class="my-2 flex">{t('paidLetterHint')}</Checkbox>
    <button class={buttonClass('primary')} disabled={busy || !recipient.trim() || !text.trim()} onclick={send}>{t('send')}</button>
    {#if sent}
      <p data-testid="letter-state">{t('order')} <span class="mono">{sent.order_id}</span> · {sent.state}</p>
    {/if}
  </div>
  {#if outgoing.length}
    <div class="card">
      <h2>{t('sentLetters')}</h2>
      <ul class="list">
        {#each outgoing as order (order.order_id)}
          <li>
            <div class="row spread">
              <span><span class="mono">{order.recipient}</span>{order.paid ? ` · ${t('paid')}` : ''}</span>
              <span class="row">
                <button class={buttonClass()} disabled={busy} onclick={() => check(order)}>{t('status')}</button>
                {#if order.paid}<button class={buttonClass()} disabled={busy} onclick={() => funds(order)}>{t('funds')}</button>{/if}
              </span>
            </div>
            {#if escrows[order.order_id]}
              <ul class="list" data-testid="escrow-transfers">
                {#each escrows[order.order_id].transfers as leg (leg.leg_id)}
                  <li class="row spread">
                    <span><span class="badge">{leg.status}</span> {leg.kind} · {leg.amount} → <span class="mono">{leg.to}</span></span>
                    {#if ['FeeBlocked', 'Rejected'].includes(leg.status)}
                      <button class={buttonClass()} disabled={busy} onclick={() => revise(order, leg)}>{t('revise')}</button>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}
  <div class="card">
    <h2>{t('contactPolicy')}</h2>
    <p class="muted">{policy ? `${t('current')}: ${policy.mode} · v${policy.version}` : t('noPolicy')}</p>
    <div class="row">
      <Select
        bind:value={mode}
        testid="policy-mode"
        label={t('contactPolicy')}
        options={[
          { value: 'closed', label: t('closed') },
          { value: 'contacts', label: t('contactsOnly') },
          { value: 'free', label: t('free') },
          { value: 'paid', label: t('paidOnly') }
        ]} />
      {#if mode === 'paid'}<input placeholder={t('priceUnits')} bind:value={price} />{/if}
      <button class={buttonClass()} disabled={busy || (mode === 'paid' && !/^\d+$/.test(price.trim()))} onclick={configure}>{t('save')}</button>
    </div>
  </div>
{/if}

<style>
  .unread {
    font-weight: 600;
  }
</style>
