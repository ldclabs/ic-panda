<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { call, code, when } from '../rpc'

  let { status, refresh, browsers }: { status: any; refresh: () => Promise<void>; browsers: boolean } = $props()
  let account = $state<any>(null)
  let pairings = $state<any[]>([])
  let invite = $state<any>(null)
  let candidates = $state<any[]>([])
  let error = $state('')
  let timer: ReturnType<typeof setInterval> | undefined

  async function load() {
    error = ''
    try {
      account = await call('account/show')
      pairings = await call('connect/pairings')
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  async function login() {
    await window.dmsg.login('https://dmsg.net')
  }

  async function act(method: string, params?: unknown) {
    try {
      await call(method, params)
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
    await refresh()
  }

  async function pair(copy: boolean) {
    invite = await window.dmsg.pair(copy)
    candidates = []
    clearInterval(timer)
    timer = setInterval(async () => {
      if (!invite || Date.now() > invite.expires_at) return clearInterval(timer)
      const found = await call('connect/candidates', { invite: invite.invite_id }).catch(() => null)
      if (found) candidates = found.candidates
    }, 2000)
  }

  /** The person compares the label with the browser that opened the link. */
  async function grant(candidate: any) {
    await window.dmsg.grant(invite.invite_id, candidate.pairing_id)
    clearInterval(timer)
    invite = null
    candidates = []
    await load()
  }

  onMount(() => {
    void load()
    return () => clearInterval(timer)
  })
  const own = $derived(account?.devices?.find((d: any) => d.current))
</script>

<h1>{t('settings')}</h1>
{#if error}<p class="error">{error}</p>{/if}
<div class="card">
  <h2>{t('status')}</h2>
  <dl>
    <dt>{t('status')}</dt>
    <dd data-testid="session-state">{status?.state ?? '-'}</dd>
    <dt>{t('account')}</dt>
    <dd class="mono">{status?.account ?? '-'}</dd>
    <dt>{t('device')}</dt>
    <dd class="mono">{status?.device_id ?? '-'}</dd>
    <dt>{t('formal')}</dt>
    <dd>{own?.capabilities?.includes('FormalApprove') ? t('formalOn') : t('formalOff')}</dd>
    <dt>sync</dt>
    <dd>{status?.pending_sync ?? 0} pending · {when(status?.last_sync?.at)}</dd>
    <dt>background online</dt>
    <dd>{status?.background_online ? 'on' : 'off'}{status?.resident ? ' (device key resident)' : ''}</dd>
  </dl>
  <div class="row" style="margin-top: 10px">
    <button onclick={login}>{t('login')}</button>
    <button onclick={() => act('session/unlock', { fast: true })}>{t('fastUnlock')}</button>
    <button onclick={() => act('session/lock')}>{t('lock')}</button>
    <button onclick={() => act('sync/run')}>{t('sync')}</button>
  </div>
</div>
<div class="card" class:focus={browsers}>
  <h2>{t('browsers')}</h2>
  <ul class="list" data-testid="pairings">
    {#each pairings as p (p.pairing_id)}
      <li class="row spread">
        <span><span class="badge">{p.state}</span> {p.label ?? p.pairing_id.slice(0, 12)}</span>
        <span class="muted">{when(p.last_used_at)}</span>
        {#if p.state === 'active'}<button class="danger" onclick={() => act('connect/revoke', { pairing: p.pairing_id })}>{t('revoke')}</button>{/if}
      </li>
    {/each}
  </ul>
  <div class="row" style="margin-top: 10px">
    <button class="primary" onclick={() => pair(false)}>{t('connectBrowser')}</button>
    <button onclick={() => pair(true)}>{t('copyLink')}</button>
  </div>
  {#if invite}
    <p class="mono">{invite.url}</p>
    {#each candidates as c (c.pairing_id)}
      <div class="row spread card">
        <span>{t('confirmBrowser')} <strong>{c.label}</strong> <span class="mono muted">{c.pairing_id.slice(0, 12)}</span></span>
        <button class="primary" onclick={() => grant(c)}>{t('connect')}</button>
      </div>
    {/each}
  {/if}
</div>

<style>
  .focus {
    border-color: var(--green);
  }
</style>
