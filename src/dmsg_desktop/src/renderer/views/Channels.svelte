<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

  let { status }: { status: any } = $props()
  let channels = $state<any[]>([])
  let selected = $state<any>(null)
  let messages = $state<any[]>([])
  let text = $state('')
  let name = $state('')
  let kind = $state('collaboration')
  let account = $state('')
  let invitation = $state('')
  let joining = $state('')
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

  async function load() {
    await attempt(async () => {
      channels = await call('channel/list')
      if (selected) await open(selected.id)
    })
  }

  async function open(id: string) {
    selected = await call('channel/show', { id })
    messages = await call('channel/messages', { id })
  }

  const read = () => attempt(async () => {
    await call('channel/sync', { id: selected.id })
    await open(selected.id)
  })

  const send = () => attempt(async () => {
    if (!text.trim()) return
    await call('channel/send', { id: selected.id, text })
    text = ''
    await open(selected.id)
  })

  const create = () => attempt(async () => {
    if (!name.trim()) return
    const created = await call('channel/create', { name, type: kind })
    name = ''
    channels = await call('channel/list')
    await open(created.id)
  })

  const invite = () => attempt(async () => {
    invitation = JSON.stringify(await call('channel/invite', { id: selected.id, account: account.trim() }))
    account = ''
  })

  const join = () => attempt(async () => {
    const ledger = await call('channel/join', { invitation: JSON.parse(joining) })
    joining = ''
    channels = await call('channel/list')
    await open(ledger.channel_id)
  })

  const rotate = () => attempt(async () => {
    selected = await call('channel/rotate', { id: selected.id })
  })

  const remove = (member: string) => attempt(async () => {
    await call('channel/control', { id: selected.id, action: { type: 'remove', account: member } })
    await open(selected.id)
  })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['channel/changed', 'session/changed', 'sync/changed'].includes(event.type)) void load()
    })
  })

  const unlocked = $derived(status?.state === 'unlocked')
  const ledger = $derived(selected?.ledger)
  const me = $derived(status?.account)
</script>

<h1>{t('channels')}</h1>
{#if error}<p class="error" data-testid="channel-error">{error}</p>{/if}
<div class="columns">
  <section>
    <ul class="list card" data-testid="channel-list">
      {#each channels as channel (channel.id)}
        <li>
          <button class="link" class:active={selected?.id === channel.id} onclick={() => attempt(() => open(channel.id))}>
            <strong>{channel.name}</strong>
            <span class="muted">{channel.ledger ? `${t('epoch')} ${channel.ledger.epoch} · ${channel.ledger.status}` : '…'}</span>
          </button>
        </li>
      {:else}
        <li class="muted">{t('noChannels')}</li>
      {/each}
    </ul>
    {#if unlocked}
      <div class="card">
        <h2>{t('newChannel')}</h2>
        <input placeholder={t('channelName')} bind:value={name} data-testid="channel-name" />
        <select bind:value={kind} style="margin-top: 8px">
          <option value="collaboration">{t('collaboration')}</option>
          <option value="distribution">{t('distribution')}</option>
          <option value="direct">{t('direct')}</option>
        </select>
        <button class="primary" style="margin-top: 8px" disabled={busy} onclick={create}>{t('create')}</button>
      </div>
      <div class="card">
        <h2>{t('joinChannel')}</h2>
        <textarea placeholder={t('invitation')} bind:value={joining}></textarea>
        <button style="margin-top: 8px" disabled={busy || !joining.trim()} onclick={join}>{t('join')}</button>
      </div>
    {/if}
  </section>
  <section>
    {#if selected}
      <div class="card">
        <div class="row spread">
          <h2>{selected.name}</h2>
          <button disabled={busy} onclick={read}>{t('read')}</button>
        </div>
        {#if ledger}
          <p class="muted">
            {t('epoch')} {ledger.epoch} · {ledger.status}{ledger.owner === me ? ` · ${t('owner')}` : ''}
          </p>
          {#if ledger.status === 'rotation_required'}
            <p class="warn">{t('rotationRequired')} <button disabled={busy} onclick={rotate}>{t('rotate')}</button></p>
          {/if}
        {/if}
        <ul class="list" data-testid="channel-messages">
          {#each messages as message (message.seq)}
            <li>
              <span class="muted mono">{message.account === me ? t('you') : message.account}</span>
              <span class="muted">{when(message.stored_at)}</span>
              <div>{message.text}{message.file ? ` [${message.file.name}]` : ''}</div>
            </li>
          {:else}
            <li class="muted">{t('noMessages')}</li>
          {/each}
        </ul>
        {#if unlocked && ledger?.status === 'active'}
          <div class="row" style="margin-top: 8px">
            <input placeholder={t('message')} bind:value={text} data-testid="channel-text" />
            <button class="primary" disabled={busy} onclick={send}>{t('send')}</button>
          </div>
        {/if}
      </div>
      {#if ledger}
        <div class="card">
          <h2>{t('members')}</h2>
          <ul class="list">
            {#each Object.values(ledger.members) as any[] as member (member.account)}
              <li class="row spread">
                <span><span class="mono">{member.account}</span> · {member.role} · {t('epoch')} ≥ {member.joined_epoch}</span>
                {#if ledger.owner === me && member.account !== me}
                  <button class="danger" disabled={busy} onclick={() => remove(member.account)}>{t('remove')}</button>
                {/if}
              </li>
            {/each}
          </ul>
          {#if unlocked && ledger.members[me] && ['owner', 'admin'].includes(ledger.members[me].role)}
            <div class="row" style="margin-top: 8px">
              <input placeholder={t('accountId')} bind:value={account} />
              <button disabled={busy || !account.trim()} onclick={invite}>{t('invite')}</button>
            </div>
            {#if invitation}
              <p class="muted">{t('invitationHint')}</p>
              <textarea readonly data-testid="channel-invitation">{invitation}</textarea>
            {/if}
          {/if}
        </div>
      {/if}
    {:else}
      <p class="muted">{t('pickChannel')}</p>
    {/if}
  </section>
</div>

<style>
  .columns {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 16px;
    align-items: start;
  }
  .link {
    border: 0;
    background: transparent;
    padding: 0;
    display: flex;
    flex-direction: column;
    text-align: left;
    width: 100%;
  }
  .link.active strong {
    color: var(--green);
  }
</style>
