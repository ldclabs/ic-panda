<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Notice from '../components/page/Notice.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { buttonClass } from '../components/ui'
  import { toast } from '../components/toast.svelte'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, single } from '../rpc'
  import AgentIds from './agents/AgentIds.svelte'
  import Audit from './agents/Audit.svelte'
  import Capabilities from './agents/Capabilities.svelte'
  import Discourse from './agents/Discourse.svelte'
  import Events from './agents/Events.svelte'
  import Knowledge from './agents/Knowledge.svelte'
  import Mail from './agents/Mail.svelte'
  import Overview from './agents/Overview.svelte'

  /**
   * Agents in parts: the account's principal with its controllers and
   * credentials, the hosted Agent IDs, what each may reach, what their
   * connectors hold of Agent Discourse, Agent Mail and Knowledge, MCP
   * Events endpoints, and the audit record. Each part loads on its own, so
   * the Agent IDs show while locked or offline; owner changes come back
   * through the daemon's `agent/changed` or `approval/changed`.
   */
  let { status, route, go }: { status: any; route: string; go: (path: string) => void } = $props()

  const tabsWithAgent = ['capabilities', 'discourse', 'mail', 'knowledge']
  const tab = $derived.by(() => {
    const part = route.split('?')[0].split('/')[2]
    return [...tabsWithAgent, 'overview', 'events', 'audit'].includes(part) ? part : 'ids'
  })
  const routeAgent = $derived.by(() => {
    if (!tabsWithAgent.includes(tab)) return ''
    const encoded = route.split('?')[0].split('/')[3] ?? ''
    try { return decodeURIComponent(encoded) } catch { return encoded }
  })
  let inspectedAgent = $state('')
  const inspect = (next: string, id: string) => {
    inspectedAgent = id
    go('/agents/' + next + '/' + encodeURIComponent(id))
  }
  const selectAgent = (id: string) => inspect(tab, id)
  function showTab(next: string) {
    const id = routeAgent || inspectedAgent
    if (id && tabsWithAgent.includes(next)) inspect(next, id)
    else go(next === 'ids' ? '/agents' : '/agents/' + next)
  }
  let principal = $state<any>(null)
  let errors = $state<Record<string, string>>({})
  let loaded = $state<Record<string, boolean>>({})
  let requiresUnlock = $state<Record<string, boolean>>({})
  let alive = true
  let generation = 0
  let lastSession: string | undefined
  let credentials = $state<any[]>([])
  let delegations = $state<any[]>([])
  let hosted = $state<any>({ agents: [], hosts: [], vocabulary: [], blocked_senders: [] })
  let quota = $state<{ used: number; limit: number | null } | null>(null)
  let grants = $state<any[]>([])
  /** An Agent ID the principal tab grants a credential to next. */
  let delegate = $state('')
  let focusDelegate = $state(false)
  let focusLabel = $state(false)
  function createFromEmpty() { focusLabel = true; showTab('ids') }
  $effect(() => {
    if (focusLabel && tab === 'ids' && loaded.hosted && unlocked) {
      focusLabel = false
      void tick().then(() => document.getElementById('hosted-agent-label')?.focus())
    }
  })
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)

  $effect(() => {
    if (!loaded.hosted) return
    if (routeAgent && hosted.agents.some((a: any) => a.agent_id === routeAgent)) inspectedAgent = routeAgent
    else if (inspectedAgent && !hosted.agents.some((a: any) => a.agent_id === inspectedAgent)) inspectedAgent = ''
  })

  const current = (turn: number) => alive && turn === generation
  const selection = $derived(routeAgent || inspectedAgent || hosted.active || hosted.agents[0]?.agent_id || '')
  const missingAgent = $derived(!!routeAgent && loaded.hosted && !errors.hosted && !hosted.agents.some((a: any) => a.agent_id === routeAgent))
  const needsHosted = $derived(tab === 'ids' || tabsWithAgent.includes(tab))
  let prepared = $state<{ id: string; action: string } | null>(null)
  let openFailure = $state('')
  async function openPrepared(receipt: { id: string; action: string }, turn: number) {
    try {
      await bridge(() => window.dmsg.openApproval(receipt.id))
      if (current(turn)) { prepared = null; openFailure = '' }
    } catch (e) {
      if (current(turn)) { prepared = receipt; openFailure = errorText(e) }
    }
  }

  async function attempt(work: () => Promise<unknown>, failed?: (message: string) => void) {
    if (busy) return false
    const turn = generation
    busy = true
    try {
      await work()
      return current(turn)
    } catch (e) {
      if (current(turn)) { if (failed) failed(errorText(e)); else toast(() => errorText(e)) }
      return false
    } finally { if (current(turn)) busy = false }
  }

  function reader(key: string, method: string, apply: (value: any) => void) {
    return single(async () => {
      const visible = key === 'hosted' || (key === 'grants' ? tab === 'capabilities' : tab === 'overview')
      if (!alive || !visible || (['credentials', 'delegations'].includes(key) && !unlocked)) return
      const turn = generation
      try {
        const value = await call(method)
        if (current(turn)) { apply(value); loaded[key] = true; requiresUnlock[key] = false; errors[key] = '' }
      } catch (e) {
        if (current(turn)) {
          requiresUnlock[key] = !unlocked && code(e) === 'LOCKED'
          errors[key] = requiresUnlock[key] ? '' : errorText(e) || t('agentLoadFailed')
        }
      }
    })
  }
  const loadHosted = reader('hosted', 'agent/ids', (value) => (hosted = value))
  const loadGrants = reader('grants', 'grants/list', (value) => (grants = value))
  const loadPrincipal = reader('principal', 'agent/principal', (value) => (principal = value))
  const loadCredentials = reader('credentials', 'agent/credentials', (value) => (credentials = value))
  const loadDelegations = reader('delegations', 'agent/delegations', (value) => (delegations = value))

  const loadQuota = single(async () => {
    if (!alive || !unlocked || tab !== 'ids') return
    const turn = generation
    try {
      const result = await call<{ used: number; limit: number | null }>('agent/quota')
      if (current(turn)) quota = result
    } catch {
      if (current(turn)) quota = null
    }
  })

  function loadTab() {
    if (tab === 'ids') void loadQuota()
    else if (tab === 'overview') {
      void loadPrincipal()
      if (unlocked) { void loadCredentials(); void loadDelegations() }
    } else if (tab === 'capabilities') void loadGrants()
  }

  async function createRequest(method: string, params: Record<string, unknown> = {}) {
    const turn = generation
    const action = JSON.stringify([method, params])
    const receipt = prepared?.action === action ? prepared : { id: (await call<{ id: string }>(method, params)).id, action }
    if (current(turn)) await openPrepared(receipt, turn)
  }
  const request = (method: string, params?: Record<string, unknown>, failed?: (message: string) => void) => attempt(() => createRequest(method, params), failed)
  const run = (method: string, params: Record<string, unknown> = {}, failed?: (message: string) => void) => attempt(() => call(method, params), failed)

  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'resync' || event.type === 'agent/changed') { void loadHosted(); loadTab() }
      else if (event.type === 'approval/changed') loadTab()
    })
    return () => { alive = false; generation++; off() }
  })

  $effect(() => {
    const next = sessionKey
    tab
    untrack(() => {
      if (lastSession !== next) {
        if (lastSession !== undefined) inspectedAgent = ''
        lastSession = next
        generation++
        principal = null
        credentials = []; delegations = []; grants = []
        hosted = { agents: [], hosts: [], vocabulary: [], blocked_senders: [] }
        quota = null; errors = {}; loaded = {}; requiresUnlock = {}; busy = false; delegate = ''; focusDelegate = false; focusLabel = false; prepared = null; openFailure = ''
        void loadHosted()
      }
      loadTab()
    })
  })

  function delegateTo(agent: string) {
    delegate = agent
    focusDelegate = true
    showTab('overview')
  }

  const tabs = $derived([
    { value: 'ids', label: t('tabAgentIds'), icon: 'agents' as const, count: hosted.agents.length },
    { value: 'capabilities', label: t('tabCapabilities'), icon: 'security' as const },
    { value: 'overview', label: t('tabPrincipal'), icon: 'plan' as const },
    { value: 'discourse', label: t('tabDiscourse'), icon: 'members' as const },
    { value: 'mail', label: t('mail'), icon: 'mail' as const },
    { value: 'knowledge', label: t('knowledge'), icon: 'knowledge' as const },
    { value: 'events', label: t('tabEvents'), icon: 'webhook' as const },
    { value: 'audit', label: t('tabAudit'), icon: 'audit' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, showTab} items={tabs} label={t('agents')} testid="agents-tab" />
  {#if prepared && unlocked}
    <Notice class="mx-4 mt-2 shrink-0" tone="warn" testid="agent-prepared">{openFailure}
      {#snippet action()}<button class={buttonClass('secondary', 'sm')} data-testid="agent-reopen" onclick={() => prepared && openPrepared(prepared, generation)}>{t('workflow.review')}</button>{/snippet}
    </Notice>
  {/if}
  <div class="min-h-0 flex-1">
    {#key sessionKey}
    {#key tab}
    <Page wide={['overview', 'capabilities', 'discourse', 'mail', 'knowledge'].includes(tab)}>
      {#if errors.hosted}
        <Notice tone="danger" testid="hosted-error">{errors.hosted}
          {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={loadHosted}>{t('retry')}</button>{/snippet}
        </Notice>
      {/if}
      {#if (needsHosted && requiresUnlock.hosted) || (tab === 'capabilities' && requiresUnlock.grants)}
        <Locked {status} testid="agents-resource-locked" />
      {:else if needsHosted && !loaded.hosted}
        {#if !errors.hosted}<p role="status" class="text-[13px] text-muted">{t('loading')}</p>{/if}
      {:else if missingAgent}
        <Notice testid="agent-missing">{t('tabAgentIds')} · {t('noResults')} <bdi class="font-mono">{routeAgent}</bdi>
          {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={() => showTab('ids')}>{t('tabAgentIds')}</button>{/snippet}
        </Notice>
      {:else if ['mail', 'knowledge', 'discourse'].includes(tab) && !hosted.agents.length}
        <Notice testid="agent-empty-tools">{t('noAgents')}
          {#snippet action()}{#if unlocked}<button class={buttonClass('secondary', 'sm')} onclick={createFromEmpty}>{t('create')}</button>{/if}{/snippet}
        </Notice>
      {:else if tab === 'overview'}
        <Overview {principal} principalError={errors.principal ?? ''} {loaded} {errors} {credentials} {delegations} {hosted} {unlocked} {busy} {request} {createRequest} {run} onRetry={loadTab} onReviewRequests={() => go('/approvals')} bind:delegate bind:focusDelegate />
      {:else if tab === 'ids'}
        <AgentIds {hosted} {quota} {unlocked} {busy} {run} {attempt} {delegateTo} onReview={loadHosted} manage={(id) => inspect('capabilities', id)} />
      {:else if tab === 'capabilities'}
        {#if errors.grants}<Notice tone="danger" testid="grants-error">{errors.grants}
          {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={loadGrants}>{t('retry')}</button>{/snippet}
        </Notice>{/if}
        {#if loaded.grants}
          {#if !hosted.agents.length && unlocked}<button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={createFromEmpty}>{t('create')} · {t('tabAgentIds')}</button>{/if}
          <Capabilities {hosted} {grants} {unlocked} {busy} {attempt} onReview={loadGrants} active={routeAgent || inspectedAgent} {selectAgent} />
        {:else if !errors.grants}<p role="status" class="text-[13px] text-muted">{t('loading')}</p>{/if}
      {:else if tab === 'discourse'}
        <Discourse agents={hosted.agents} active={selection} {selectAgent} />
      {:else if tab === 'mail'}
        {#key route}
          <Mail agents={hosted.agents} active={selection} {selectAgent} blocked={hosted.blocked_senders ?? []} {unlocked} manage={() => inspect('capabilities', selection)} />
        {/key}
      {:else if tab === 'knowledge'}
        <Knowledge agents={hosted.agents} active={selection} {selectAgent} hosts={hosted.hosts} {unlocked} manage={() => inspect('capabilities', selection)} />
      {:else if tab === 'events'}
        <Events agents={hosted.agents} {unlocked} />
      {:else}
        <Audit agents={hosted.agents} />
      {/if}
    </Page>
    {/key}
    {/key}
  </div>
</div>
