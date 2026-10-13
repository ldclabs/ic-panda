<script lang="ts">
  import { onMount } from 'svelte'
  import { t, stateLabel } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'

  let { status }: { status: any } = $props()
  let requests = $state<any[]>([])
  let history = $state<any[]>([])
  let usage = $state<any>(null)
  let error = $state('')
  let notice = $state('')
  let text = $state('')
  let report = $state<any>(null)

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

  /** Refreshes the month's allowance from the plan and archives finished months here. */
  async function loadUsage() {
    try {
      usage = await call('attest/usage')
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  async function enableFormal() {
    const request = await call('device/formal_approve', { enable: true })
    await window.dmsg.openApproval(request.id)
  }

  /** Files are picked in the main process's dialogs; the window sees only the outcome. */
  async function picked(work: () => Promise<void>) {
    error = ''
    notice = ''
    try {
      await work()
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  /** An action its application prepared, approved in the approval window like a page's `signAction`. */
  const signAction = () =>
    picked(async () => {
      const request: any = await window.dmsg.attestAction()
      if (request) await window.dmsg.openApproval(request.id)
    })

  const verify = (withFile: boolean) =>
    picked(async () => {
      report = (await window.dmsg.verifyBundle(withFile)) ?? report
    })

  const exportBundle = (execution: string) =>
    picked(async () => {
      const saved = await window.dmsg.exportBundle(execution)
      if (saved) notice = `${t('bundleSaved')}: ${saved.path}`
    })

  /** What a statement says, in one line. */
  const summary = (s: any) =>
    s.profile === 'app_action' ? `${s.content.app_id} · ${s.content.command}` : (s.content.text ?? s.content.sha256)

  const label = (r: any) =>
    r.type === 'connect'
      ? `${r.detail.app_id} · ${r.detail.origin}`
      : r.type === 'attest'
        ? `${r.detail.statement.profile}: ${summary(r.detail.statement)}`
        : r.type

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (event.type === 'approval/changed' || event.type === 'session/changed') void load()
    })
  })
</script>

<h1>{t('approvals')}</h1>
{#if error}<p class="muted" data-testid="approvals-error">{error}</p>{/if}
{#if notice}<p class="muted" data-testid="approvals-notice">{notice}</p>{/if}
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
    <textarea placeholder={t('statement')} bind:value={text} data-testid="statement-text"></textarea>
    <div class="row" style="margin-top: 8px">
      <button class="primary" onclick={sign} data-testid="statement-submit">{t('submit')}</button>
      <button onclick={enableFormal}>{t('formal')}: {t('enable')}</button>
    </div>
    <h3 style="margin-top: 12px">{t('appAction')}</h3>
    <p class="muted">{t('appActionHint')}</p>
    <button data-testid="sign-action" onclick={signAction}>{t('appActionPick')}</button>
  </div>
{/if}
<div class="card" data-testid="verify-bundle">
  <h2>{t('verifyBundle')}</h2>
  <p class="muted">{t('verifyBundleHint')}</p>
  <div class="row">
    <button onclick={() => verify(false)}>{t('verifyBundle')}</button>
    <button onclick={() => verify(true)}>{t('verifyWithFile')}</button>
  </div>
  {#if report}
    <dl style="margin-top: 8px" data-testid="verify-report">
      <dt>{t('content')}</dt>
      <dd>{report.content}</dd>
      <dt>{t('signature')}</dt>
      <dd>{report.signature ? t('valid') : t('invalid')} · <span class="mono">{report.key_fingerprint}</span></dd>
      <dt>{t('account')}</dt>
      <dd class="mono">{report.account}</dd>
      <dt>{t('device')}</dt>
      <dd><span class="mono">{report.device_id}</span> · {report.current}</dd>
      <dt>{t('approvedAt')}</dt>
      <dd>{when(report.approved_at)} · {t('securityEpoch')} {report.security_epoch}</dd>
      <dt>{t('origin')}</dt>
      <dd>{report.origin}</dd>
      <dt>{t('certifiedAt')}</dt>
      <dd>{when(report.certified_at)}</dd>
      <dt>{t('statement')}</dt>
      <dd>{report.statement?.profile}: {summary(report.statement)}</dd>
    </dl>
  {/if}
</div>
{#if status?.state === 'unlocked'}
  <div class="card">
    <div class="row spread">
      <h2>{t('usage')}</h2>
      <button onclick={loadUsage}>{t('refresh')}</button>
    </div>
    {#if usage}
      <p data-testid="usage">{usage.charged} / {usage.allowed} · {usage.remaining} {t('remaining')} · {t('validUntil')} {when(usage.valid_until_ms)}</p>
      {#if usage.history.length}
        <h3>{t('archived')}</h3>
        <ul class="list">
          {#each usage.history as month (month.month)}
            <li class="row spread"><span>{month.month}</span><span class="muted">{month.charged} / {month.allowed}</span></li>
          {/each}
        </ul>
      {/if}
    {/if}
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
        <div class="row spread">
          <span class="badge">{entry.stage}</span>
          <span class="row">
            <span class="muted">{when(entry.created_at)}</span>
            {#if entry.stage === 'complete'}<button onclick={() => exportBundle(entry.execution)}>{t('exportBundle')}</button>{/if}
          </span>
        </div>
        <div>{summary(entry.statement)}</div>
        <div class="mono muted">{entry.origin} · {entry.execution}</div>
      </li>
    {/each}
  </ul>
</div>
