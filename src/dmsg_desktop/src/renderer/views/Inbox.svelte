<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

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

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['inbox/changed', 'session/changed', 'approval/changed'].includes(event.type)) void load()
    })
  })

  const unlocked = $derived(status?.state === 'unlocked')
  const shown = $derived(letters.filter((l) => !l.archived))
</script>

<h1>{t('inbox')}</h1>
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
        {#if !letter.read}<button disabled={busy} onclick={() => mark(letter, false)}>{t('markRead')}</button>{/if}
        <button disabled={busy} onclick={() => mark(letter, true)}>{t('archive')}</button>
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
    <label class="row" style="margin: 8px 0"><input type="checkbox" bind:checked={pay} /> {t('paidLetterHint')}</label>
    <button class="primary" disabled={busy || !recipient.trim() || !text.trim()} onclick={send}>{t('send')}</button>
    {#if sent}
      <p data-testid="letter-state">{t('order')} <span class="mono">{sent.order_id}</span> · {sent.state}</p>
    {/if}
  </div>
  {#if outgoing.length}
    <div class="card">
      <h2>{t('sentLetters')}</h2>
      <ul class="list">
        {#each outgoing as order (order.order_id)}
          <li class="row spread">
            <span><span class="mono">{order.recipient}</span>{order.paid ? ` · ${t('paid')}` : ''}</span>
            <button disabled={busy} onclick={() => check(order)}>{t('status')}</button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}
  <div class="card">
    <h2>{t('contactPolicy')}</h2>
    <p class="muted">{policy ? `${t('current')}: ${policy.mode} · v${policy.version}` : t('noPolicy')}</p>
    <div class="row">
      <select bind:value={mode} data-testid="policy-mode">
        <option value="closed">{t('closed')}</option>
        <option value="contacts">{t('contactsOnly')}</option>
        <option value="free">{t('free')}</option>
        <option value="paid">{t('paidOnly')}</option>
      </select>
      {#if mode === 'paid'}<input placeholder={t('priceUnits')} bind:value={price} />{/if}
      <button disabled={busy || (mode === 'paid' && !/^\d+$/.test(price.trim()))} onclick={configure}>{t('save')}</button>
    </div>
  </div>
{/if}

<style>
  .unread {
    font-weight: 600;
  }
</style>
