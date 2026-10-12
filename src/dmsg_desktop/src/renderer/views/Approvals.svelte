<script lang="ts">
  import { onMount } from 'svelte'
  import { t, stateLabel } from '../i18n'
  import { call, code, when } from '../rpc'

  let { status }: { status: any } = $props()
  let requests = $state<any[]>([])
  let history = $state<any[]>([])
  let error = $state('')
  let text = $state('')

  async function load() {
    error = ''
    try {
      requests = await call('approval/list', { all: true })
      history = await call('attest/history')
    } catch (e) {
      requests = []
      history = []
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    }
  }

  async function sign() {
    if (!text.trim()) return
    const request = await call('attest/request', { statement: { profile: 'text', text } })
    text = ''
    await window.dmsg.openApproval(request.id)
    await load()
  }

  async function enableFormal() {
    const request = await call('device/formal_approve', { enable: true })
    await window.dmsg.openApproval(request.id)
  }

  const label = (r: any) =>
    r.type === 'connect'
      ? `${r.detail.app_id} · ${r.detail.origin}`
      : r.type === 'attest'
        ? `${r.detail.statement.profile}: ${r.detail.statement.content.text ?? r.detail.statement.content.sha256}`
        : r.type

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (event.type === 'approval/changed' || event.type === 'session/changed') void load()
    })
  })
</script>

<h1>{t('approvals')}</h1>
{#if error}<p class="muted">{error}</p>{/if}
<div class="card">
  <h2>{t('waiting')}</h2>
  <ul class="list" data-testid="waiting-list">
    {#each requests.filter((r) => r.state === 'awaiting_user') as request (request.id)}
      <li class="row spread">
        <span><span class="badge">{request.source}</span> {label(request)}</span>
        <button class="primary" onclick={() => window.dmsg.openApproval(request.id)}>{t('open')}</button>
      </li>
    {:else}
      <li class="muted">{t('noRequests')}</li>
    {/each}
  </ul>
</div>
{#if status?.state === 'unlocked'}
  <div class="card">
    <h2>{t('newStatement')}</h2>
    <textarea placeholder={t('statement')} bind:value={text}></textarea>
    <div class="row" style="margin-top: 8px">
      <button class="primary" onclick={sign}>{t('submit')}</button>
      <button onclick={enableFormal}>{t('formal')}: {t('enable')}</button>
    </div>
  </div>
{/if}
<div class="card">
  <h2>{t('history')}</h2>
  <ul class="list">
    {#each requests.filter((r) => r.state !== 'awaiting_user') as request (request.id)}
      <li class="row spread">
        <span><span class="badge">{stateLabel(request.state)}</span> {label(request)}</span>
        <span class="muted">{when(request.created_at)}</span>
      </li>
    {/each}
  </ul>
</div>
<div class="card">
  <h2>{t('signed')}</h2>
  <ul class="list">
    {#each history as entry (entry.execution)}
      <li>
        <div class="row spread"><span class="badge">{entry.stage}</span><span class="muted">{when(entry.created_at)}</span></div>
        <div>{entry.statement.content.text ?? entry.statement.content.sha256}</div>
        <div class="mono muted">{entry.origin} · {entry.execution}</div>
      </li>
    {/each}
  </ul>
</div>
