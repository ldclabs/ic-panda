<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import AgentMail from './AgentMail.svelte'
  import AgentEvents from './AgentEvents.svelte'
  import AgentAudit from './AgentAudit.svelte'
  import Select from '../components/Select.svelte'

  let { status }: { status: any } = $props()
  let principal = $state<any>(null)
  let credentials = $state<any[]>([])
  let hosted = $state<any>({ agents: [], hosts: [] })
  let grants = $state<any[]>([])
  let error = $state('')
  let busy = $state(false)
  let kind = $state('person')
  let controllerName = $state('')
  let ceilingScopes = $state('')
  let ceilingAudiences = $state('')
  let subject = $state('')
  let scopes = $state('')
  let audiences = $state('')
  let days = $state(30)
  let label = $state('')
  let issued = $state<{ agent_id: string; credential: string } | null>(null)
  let grantee = $state('')
  let capabilities = $state('')

  /** What an agent may be granted; `<name>:*` covers every target. */
  const VOCABULARY =
    'account.read vault.list vault.read:<tag> vault.write:<tag> vault.secret:<item> file.read:<tag> channel.read:<channel> channel.publish:<channel> inbox.read inbox.contact profile.read attest.request action.request agent.delegation.read events.endpoint'

  const list = (text: string, separator: RegExp) =>
    text
      .split(separator)
      .map((v) => v.trim())
      .filter(Boolean)

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
      hosted = await call('agent/ids')
      principal = await call('agent/principal')
      grants = await call('grants/list')
      credentials = principal ? await call('agent/credentials') : []
    })

  /** Principal changes and grants are approval requests the approval window decides. */
  const request = (method: string, params: Record<string, unknown> = {}) =>
    attempt(async () => {
      const created: any = await call(method, params)
      await window.dmsg.openApproval(created.id)
    })

  const register = () =>
    request('agent/register', {
      name: controllerName.trim() || null,
      ...(ceilingScopes.trim() || ceilingAudiences.trim()
        ? { scopes: list(ceilingScopes, /,/), audiences: list(ceilingAudiences, /,/) }
        : {})
    })

  const grant = () =>
    request('agent/grant', { subject: subject.trim(), scopes: list(scopes, /,/), audiences: list(audiences, /,/), days })

  const run = (method: string, params: Record<string, unknown>) =>
    attempt(async () => {
      await call(method, params)
      await load()
    })

  const issue = (agent: string) =>
    attempt(async () => {
      issued = await call('agent/credential', { agent })
      hosted = await call('agent/ids')
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed', 'agent/changed'].includes(event.type)) void load()
    })
  })

  const unlocked = $derived(status?.state === 'unlocked')
  const current = $derived((principal?.controllers ?? []).filter((c: any) => !c.retired_at))
  const label_of = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did
</script>

<h1>{t('agents')}</h1>
{#if error}<p class="error" data-testid="agents-error">{error}</p>{/if}
<div class="card">
  <h2>{t('principal')}</h2>
  <p class="muted">{t('principalHint')}</p>
  {#if principal}
    <dl>
      <dt>ID</dt>
      <dd class="mono" data-testid="principal-id">{principal.principal_id}</dd>
      <dt>{t('status')}</dt>
      <dd data-testid="principal-published">
        {principal.published ? t('published') : t('unpublished')}
        {#if !principal.published}<button disabled={busy} onclick={() => run('agent/publish', {})}>{t('publish')}</button>{/if}
      </dd>
    </dl>
    <h2>{t('controllers')}</h2>
    <ul class="list" data-testid="controllers">
      {#each principal.controllers as c (c.generation)}
        <li class="row spread">
          <span>
            #{c.generation}
            {c.name ?? ''}
            <span class="mono muted">{c.agent_id}</span>
          </span>
          <span class="muted">
            {c.delegation === '*' ? t('everything') : `${c.delegation.scopes.join(', ')} → ${c.delegation.audiences.join(', ')}`}
            {#if c.retired_at}· {t('retired')}{:else if unlocked}<button disabled={busy} onclick={() => request('agent/retire', { generation: c.generation })}>{t('retire')}</button>{/if}
          </span>
        </li>
      {/each}
    </ul>
    {#if unlocked}
      <h2>{t('registerController')}</h2>
      <div class="row">
        <input placeholder={t('name')} bind:value={controllerName} style="max-width: 160px" />
        <input placeholder={t('scopes')} bind:value={ceilingScopes} />
        <input placeholder={t('audiences')} bind:value={ceilingAudiences} />
        <button class="primary" disabled={busy} onclick={register}>{t('register')}</button>
      </div>
    {/if}
  {:else if unlocked}
    <div class="row">
      <Select bind:value={kind} options={['person', 'organization', 'team', 'project', 'other'].map((k) => ({ value: k, label: k }))} />
      <button class="primary" disabled={busy} data-testid="enable-principal" onclick={() => request('agent/enable', { type: kind })}>
        {t('enablePrincipal')}
      </button>
    </div>
  {/if}
</div>
{#if principal}
  <div class="card">
    <h2>{t('credentials')}</h2>
    {#if credentials.length}
      <ul class="list" data-testid="credentials">
        {#each credentials as c (c.id)}
          <li class="row spread">
            <span>
              {label_of(c.subject)} · {c.scopes.join(', ')} → {c.audiences.join(', ')}
              <span class="muted">{c.expires_at ? `· ${when(c.expires_at)}` : ''}</span>
            </span>
            <span class="muted">
              {c.status}
              {#if c.status === 'active' && unlocked}<button disabled={busy} onclick={() => request('agent/revoke', { id: c.id })}>{t('revoke')}</button>{/if}
            </span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{t('noCredentials')}</p>
    {/if}
    {#if unlocked && current.length}
      <div class="row" style="margin-top: 8px">
        <input placeholder={t('grantee')} list="hosted-agents" bind:value={subject} data-testid="grant-subject" />
        <datalist id="hosted-agents">
          {#each hosted.agents as a (a.agent_id)}<option value={a.label}>{a.agent_id}</option>{/each}
        </datalist>
        <input placeholder={t('scopes')} bind:value={scopes} data-testid="grant-scopes" />
        <input placeholder={t('audiences')} bind:value={audiences} data-testid="grant-audiences" />
        <input type="number" min="1" max="366" bind:value={days} style="max-width: 80px" title={t('days')} />
        <button
          class="primary"
          data-testid="grant-submit"
          disabled={busy || !subject.trim() || !scopes.trim() || !audiences.trim()}
          onclick={grant}>{t('grant')}</button>
      </div>
    {/if}
  </div>
{/if}
<div class="card">
  <h2>{t('agentIds')}</h2>
  <p class="muted">{t('agentIdsHint')}</p>
  <ul class="list" data-testid="agent-ids">
    {#each hosted.agents as a (a.agent_id)}
      <li>
        <div class="row spread">
          <span>
            <strong>{a.label}</strong>
            {#if hosted.active === a.agent_id}<span class="muted">· {t('active')}</span>{/if}
            {#if a.resident}<span class="muted">· {t('resident')}</span>{/if}
          </span>
          <span class="mono muted">{a.agent_id}</span>
        </div>
        <div class="row">
          <button disabled={busy} onclick={() => run('agent/use', { agent: a.agent_id })}>{t('use')}</button>
          <button disabled={busy || !unlocked} onclick={() => run('agent/resident', { agent: a.agent_id, on: !a.resident })}>
            {a.resident ? t('residentOff') : t('residentOn')}
          </button>
          <button disabled={busy} onclick={() => issue(a.agent_id)}>{t('issueCredential')}</button>
        </div>
        {#if issued?.agent_id === a.agent_id}
          <p class="warn">{t('credentialOnce')}</p>
          <pre class="mono" data-testid="agent-credential">{issued?.credential}</pre>
        {/if}
      </li>
    {/each}
  </ul>
  {#if unlocked}
    <div class="row">
      <input placeholder={t('label')} bind:value={label} data-testid="agent-label" style="max-width: 200px" />
      <button disabled={busy || !label.trim()} onclick={() => run('agent/create', { label: label.trim() })}>{t('create')}</button>
    </div>
  {/if}
  <h2>{t('hosts')}</h2>
  <p class="muted">{t('hostsHint')}</p>
  <ul class="list">
    {#each hosted.hosts as h (h.host)}<li class="mono">{h.host}{h.label ? ` · ${h.label}` : ''}</li>{/each}
  </ul>
</div>
{#if unlocked}
  <div class="card">
    <h2>{t('capabilityGrants')}</h2>
    <p class="muted">{t('capabilitiesHint')}</p>
    <p class="muted">{t('capabilityVocabulary')}<span class="mono">{VOCABULARY}</span></p>
    <ul class="list" data-testid="capability-grants">
      {#each grants as g (g.id)}
        <li class="row spread">
          <span>{label_of(g.grantee)} · {g.capabilities.join(' ')}</span>
          <span class="muted">
            {g.revoked_at ? t('retired') : g.expires_at ? when(g.expires_at) : ''}
            {#if !g.revoked_at}<button disabled={busy} onclick={() => run('grants/revoke', { id: g.id })}>{t('revoke')}</button>{/if}
          </span>
        </li>
      {/each}
    </ul>
    <div class="row">
      <Select
        bind:value={grantee}
        placeholder={t('grantee')}
        options={hosted.agents.map((a: any) => ({ value: a.agent_id, label: a.label }))} />
      <input placeholder={t('capabilityList')} bind:value={capabilities} />
      <button
        disabled={busy || !grantee || !capabilities.trim()}
        onclick={() => run('grants/add', { agent: grantee, capabilities: list(capabilities, /\s+/) })}>{t('grant')}</button>
    </div>
  </div>
{/if}
{#if unlocked && hosted.agents.length}
  <AgentMail agents={hosted.agents} active={hosted.active} />
{/if}
{#if unlocked}
  <AgentEvents agents={hosted.agents} />
{/if}
<AgentAudit agents={hosted.agents} />
