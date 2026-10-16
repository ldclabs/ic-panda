<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, CallError, code, when } from '../rpc'
  import Select from '../components/Select.svelte'
  import Suggest from '../components/Suggest.svelte'
  import { buttonClass } from '../components/ui'

  let { status }: { status: any } = $props()
  let channels = $state<any[]>([])
  let selected = $state<any>(null)
  let messages = $state<any[]>([])
  let files = $state<any[]>([])
  let attachment = $state('')
  let text = $state('')
  let name = $state('')
  let kind = $state('collaboration')
  let account = $state('')
  let invitation = $state('')
  let joining = $state('')
  /** A transfer packet passed between the owner's and the target's devices. */
  let packet = $state('')
  let handed = $state('')
  let transferTo = $state('')
  /** The member whose history grant is being filled in; empty when none. */
  let history = $state({ account: '', from: 1, to: 1 })
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)

  const ROLES = ['admin', 'publisher', 'member']

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    notice = ''
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
    if (selected?.id !== id) {
      handed = ''
      history.account = ''
    }
    selected = await call('channel/show', { id })
    messages = await call('channel/messages', { id })
    files = (await call('vault/list', {}).catch(() => [])).filter((e: any) => e.item.type === 'file')
  }

  const read = () =>
    attempt(async () => {
      await call('channel/sync', { id: selected.id })
      await open(selected.id)
    })

  const send = () =>
    attempt(async () => {
      if (!text.trim() && !attachment) return
      if (attachment) await call('channel/send_file', { id: selected.id, item: attachment, text })
      else await call('channel/send', { id: selected.id, text })
      text = ''
      attachment = ''
      await open(selected.id)
    })

  /** A new file enters the vault through the system dialog, then goes with the next message. */
  const importFile = () =>
    attempt(async () => {
      const entry: any = await window.dmsg.importFile()
      if (!entry) return
      await open(selected.id)
      attachment = entry.id
    })

  const download = (message: any) =>
    attempt(async () => {
      const manifest: any = await window.dmsg.channelDownload(selected.id, message.seq, message.file.name)
      if (manifest) notice = `${t('verified')} · ${message.file.name} · sha256 ${String(manifest.sha256).slice(0, 16)}…`
    })

  const create = () =>
    attempt(async () => {
      if (!name.trim()) return
      const created = await call('channel/create', { name, type: kind })
      name = ''
      channels = await call('channel/list')
      await open(created.id)
    })

  const invite = () =>
    attempt(async () => {
      invitation = JSON.stringify(await call('channel/invite', { id: selected.id, account: account.trim() }))
      account = ''
    })

  const join = () =>
    attempt(async () => {
      const ledger = await call('channel/join', { invitation: JSON.parse(joining) })
      joining = ''
      channels = await call('channel/list')
      await open(ledger.channel_id)
    })

  const rotate = () =>
    attempt(async () => {
      selected = await call('channel/rotate', { id: selected.id })
    })

  const control = (action: Record<string, unknown>) =>
    attempt(async () => {
      await call('channel/control', { id: selected.id, action })
      await open(selected.id)
    })

  const leave = () =>
    attempt(async () => {
      await call('channel/leave', { id: selected.id })
      notice = t('left')
      await open(selected.id)
    })

  const reactivate = () =>
    attempt(async () => {
      selected = await call('channel/reactivate', { id: selected.id })
      await open(selected.id)
    })

  /** Older epochs' keys sealed to the member's devices. */
  const grantHistory = () =>
    attempt(async () => {
      await call('channel/history', { id: selected.id, account: history.account, from: Number(history.from), to: Number(history.to) })
      notice = `${t('historyGranted')} ${history.from}–${history.to}`
      history.account = ''
      await open(selected.id)
    })

  /** The owner proposes; the packet goes to a device of the target, which accepts and hands it back. */
  const propose = (method: 'channel/owner_propose' | 'channel/sponsor_propose') =>
    attempt(async () => {
      handed = JSON.stringify(await call(method, { id: selected.id, account: transferTo.trim() }))
      transferTo = ''
    })

  /** A packet without an acceptance is accepted here; an accepted one is committed by the owner who proposed it. */
  const settle = () =>
    attempt(async () => {
      let parsed: any
      try {
        parsed = JSON.parse(packet)
      } catch {
        throw new CallError('INVALID_INPUT: packet')
      }
      const owner = parsed?.format === 'dmsg-channel-owner-transfer/1'
      if (!owner && parsed?.format !== 'dmsg-channel-sponsor/1') throw new CallError('INVALID_INPUT: packet')
      const step = parsed.acceptance ? 'commit' : 'accept'
      const result = await call(`channel/${owner ? 'owner' : 'sponsor'}_${step}`, { packet: parsed })
      packet = ''
      handed = step === 'accept' ? JSON.stringify(result) : ''
      notice = step === 'accept' ? t('packetAccepted') : t('packetCommitted')
      channels = await call('channel/list')
      if (selected) await open(selected.id)
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
  const myRole = $derived(ledger?.members?.[me]?.role)
  const owner = $derived(ledger?.owner === me)
  const manages = $derived(owner || myRole === 'admin')
</script>

{#if error}<p class="error" data-testid="channel-error">{error}</p>{/if}
{#if notice}<p class="muted" data-testid="channel-notice">{notice}</p>{/if}
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
        <div class="row" style="margin-top: 8px">
          <Select
            bind:value={kind}
            options={[
              { value: 'collaboration', label: t('collaboration') },
              { value: 'distribution', label: t('distribution') },
              { value: 'direct', label: t('direct') }
            ]} />
          <button class={buttonClass('primary')} disabled={busy} onclick={create}>{t('create')}</button>
        </div>
      </div>
      <div class="card">
        <h2>{t('joinChannel')}</h2>
        <textarea placeholder={t('invitation')} bind:value={joining}></textarea>
        <button class={buttonClass()} style="margin-top: 8px" disabled={busy || !joining.trim()} onclick={join}>{t('join')}</button>
      </div>
      <div class="card">
        <h2>{t('transferPacket')}</h2>
        <p class="muted">{t('transferPacketHint')}</p>
        <textarea placeholder={t('transferPacket')} bind:value={packet} data-testid="channel-packet"></textarea>
        <button class={buttonClass()} style="margin-top: 8px" disabled={busy || !packet.trim()} data-testid="channel-packet-settle" onclick={settle}>{t('settlePacket')}</button>
      </div>
    {/if}
  </section>
  <section>
    {#if handed}
      <div class="card">
        <p class="warn">{t('handPacket')}</p>
        <textarea readonly data-testid="channel-handed">{handed}</textarea>
      </div>
    {/if}
    {#if selected}
      <div class="card">
        <div class="row spread">
          <h2>{selected.name}</h2>
          <button class={buttonClass()} disabled={busy} onclick={read}>{t('read')}</button>
        </div>
        {#if ledger}
          <p class="muted">
            {t('epoch')} {ledger.epoch} · {ledger.status}{owner ? ` · ${t('owner')}` : myRole ? ` · ${myRole}` : ''}
          </p>
          {#if ledger.status === 'rotation_required'}
            <p class="warn">{t('rotationRequired')} <button class={buttonClass()} disabled={busy} onclick={rotate}>{t('rotate')}</button></p>
          {/if}
          {#if ledger.status === 'archived'}
            <p class="warn">
              {t('archivedChannel')}
              {#if owner && unlocked}<button class={buttonClass()} disabled={busy} data-testid="channel-reactivate" onclick={reactivate}>{t('reactivate')}</button>{/if}
            </p>
          {/if}
        {/if}
        <ul class="list" data-testid="channel-messages">
          {#each messages as message (message.seq)}
            <li>
              <span class="muted mono">{message.account === me ? t('you') : message.account}</span>
              <span class="muted">{when(message.stored_at)}</span>
              <div>{message.text}</div>
              {#if message.file}
                <div class="row">
                  <span class="muted">📎 {message.file.name} ({t('bytes', { n: message.file.size })})</span>
                  <button class={buttonClass()} disabled={busy} onclick={() => download(message)}>{t('download')}</button>
                </div>
              {/if}
            </li>
          {:else}
            <li class="muted">{t('noMessages')}</li>
          {/each}
        </ul>
        {#if unlocked && ledger?.status === 'active'}
          <div class="row" style="margin-top: 8px">
            <input placeholder={t('message')} bind:value={text} data-testid="channel-text" />
            <button class={buttonClass('primary')} disabled={busy} onclick={send}>{t('send')}</button>
          </div>
          <div class="row" style="margin-top: 8px">
            <Select
              bind:value={attachment}
              testid="channel-attachment"
              options={[{ value: '', label: t('noAttachment') }, ...files.map((file) => ({ value: file.id, label: `📎 ${file.item.file.name}` }))]} />
            <button class={buttonClass()} disabled={busy} onclick={importFile}>{t('importFile')}</button>
          </div>
        {/if}
      </div>
      {#if ledger}
        <div class="card">
          <h2>{t('members')}</h2>
          <ul class="list" data-testid="channel-members">
            {#each Object.values(ledger.members) as any[] as member (member.account)}
              <li>
                <div class="row spread">
                  <span>
                    <span class="mono">{member.account}</span> · {member.role}{member.can_rotate ? ` · ${t('canRotate')}` : ''} · {t('epoch')} ≥ {member.joined_epoch}
                    {#if member.history.length}<span class="muted">· {t('history')} {member.history.map((h: number[]) => `${h[0]}–${h[1]}`).join(', ')}</span>{/if}
                  </span>
                  {#if unlocked && member.account !== me && member.role !== 'owner'}
                    <span class="row">
                      {#if owner}
                        <Select
                          value={member.role}
                          testid="member-role"
                          disabled={busy}
                          options={ROLES.map((role) => ({ value: role, label: role }))}
                          onChange={(role) => control({ type: 'role', account: member.account, role, can_rotate: member.can_rotate })} />
                        <button class={buttonClass()} disabled={busy} onclick={() => control({ type: 'role', account: member.account, role: member.role, can_rotate: !member.can_rotate })}>
                          {member.can_rotate ? t('noRotate') : t('allowRotate')}
                        </button>
                      {/if}
                      {#if manages && member.joined_epoch > 1}
                        <button class={buttonClass()} disabled={busy} onclick={() => (history = { account: member.account, from: 1, to: member.joined_epoch - 1 })}>{t('grantHistory')}</button>
                      {/if}
                      {#if owner}
                        <button class={buttonClass('danger')} disabled={busy} onclick={() => control({ type: 'remove', account: member.account })}>{t('remove')}</button>
                      {/if}
                    </span>
                  {/if}
                </div>
                {#if history.account === member.account}
                  <div class="row" style="margin-top: 8px" data-testid="history-form">
                    <span class="muted">{t('epochs')}</span>
                    <input type="number" min="1" bind:value={history.from} style="max-width: 90px" />
                    <span>–</span>
                    <input type="number" min="1" bind:value={history.to} style="max-width: 90px" />
                    <button class={buttonClass('primary')} disabled={busy} data-testid="history-submit" onclick={grantHistory}>{t('grantHistory')}</button>
                    <button class={buttonClass()} onclick={() => (history.account = '')}>{t('cancel')}</button>
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
          {#if unlocked && manages && ledger.status !== 'archived'}
            <div class="row" style="margin-top: 8px">
              <input placeholder={t('accountId')} bind:value={account} />
              <button class={buttonClass()} disabled={busy || !account.trim()} onclick={invite}>{t('invite')}</button>
            </div>
            {#if invitation}
              <p class="muted">{t('invitationHint')}</p>
              <textarea readonly data-testid="channel-invitation">{invitation}</textarea>
            {/if}
          {/if}
        </div>
        {#if unlocked && ledger.members[me]}
          <div class="card">
            <h2>{t('manageChannel')}</h2>
            {#if owner}
              <div class="row">
                <Suggest
                  bind:value={transferTo}
                  placeholder={t('targetMember')}
                  testid="transfer-target"
                  class="font-mono text-xs"
                  suggestions={Object.keys(ledger.members)
                    .filter((a) => a !== me)
                    .map((id) => ({ value: id }))}
                />
                <button class={buttonClass()} disabled={busy || !transferTo.trim()} data-testid="owner-propose" onclick={() => propose('channel/owner_propose')}>{t('transferOwner')}</button>
                <button class={buttonClass()} disabled={busy || !transferTo.trim()} onclick={() => propose('channel/sponsor_propose')}>{t('transferSponsor')}</button>
              </div>
              <p class="muted">{t('transferHintOwner')}</p>
              {#if ledger.status !== 'archived'}
                <button class={buttonClass('danger')} disabled={busy} data-testid="channel-archive" onclick={() => control({ type: 'archive' })}>{t('archive')}</button>
              {/if}
            {:else}
              <button class={buttonClass('danger')} disabled={busy} data-testid="channel-leave" onclick={leave}>{t('leave')}</button>
            {/if}
          </div>
        {/if}
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
    text-align: start;
    width: 100%;
  }
  .link.active strong {
    color: var(--forest);
  }
</style>
