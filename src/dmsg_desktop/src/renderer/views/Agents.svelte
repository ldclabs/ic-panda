<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Icon from '../components/Icon.svelte'
  import { cn } from '../components/ui'
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
  let { status, route }: { status: any; route: string } = $props()

  type Tab = 'overview' | 'ids' | 'capabilities' | 'discourse' | 'mail' | 'knowledge' | 'events' | 'audit'

  let tab = $state<Tab>('overview')
  let inspectedAgent = $state<string | null>(null)
  /** A malformed escape in a hand-written URL still opens Mail for that text. */
  function decoded(value: string) {
    try {
      return decodeURIComponent(value)
    } catch {
      return value
    }
  }
  $effect(() => {
    if (route.startsWith('/agents/mail/')) {
      tab = 'mail'
      inspectedAgent = decoded(route.slice('/agents/mail/'.length))
    }
  })
  let principal = $state<any>(null)
  let errors = $state<Record<string, string>>({})
  let loaded = $state<Record<string, boolean>>({})
  let alive = true
  let generation = 0
  let lastSession: string | undefined
  let credentials = $state<any[]>([])
  let delegations = $state<any[]>([])
  let hosted = $state<any>({ agents: [], hosts: [], vocabulary: [], blocked_senders: [] })
  let grants = $state<any[]>([])
  /** An Agent ID the principal tab grants a credential to next. */
  let delegate = $state('')
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)

  $effect(() => {
    if (loaded.hosted && inspectedAgent && !hosted.agents.some((agent: any) => agent.agent_id === inspectedAgent)) inspectedAgent = null
  })

  const current = (turn: number) => alive && turn === generation
  const quiet = (e: unknown) => ['LOCKED', 'AUTH_REQUIRED', 'RECOVERY_INCOMPLETE'].includes(code(e))

  async function attempt(work: () => Promise<unknown>) {
    const turn = generation
    busy = true
    try { await work() }
    catch (e) { if (current(turn)) toast(() => errorText(e)) }
    finally { if (current(turn)) busy = false }
  }

  function reader(key: string, method: string, apply: (value: any) => void) {
    return single(async () => {
      const visible = key === 'hosted' || (key === 'grants' ? tab === 'capabilities' : tab === 'overview')
      if (!alive || !visible || (['credentials', 'delegations'].includes(key) && !unlocked)) return
      const turn = generation
      try {
        const value = await call(method)
        if (current(turn)) { apply(value); errors[key] = '' }
      } catch (e) {
        if (current(turn)) errors[key] = quiet(e) ? '' : errorText(e)
      } finally {
        if (current(turn)) loaded[key] = true
      }
    })
  }
  const loadHosted = reader('hosted', 'agent/ids', (value) => (hosted = value))
  const loadGrants = reader('grants', 'grants/list', (value) => (grants = value))
  const loadPrincipal = reader('principal', 'agent/principal', (value) => (principal = value))
  const loadCredentials = reader('credentials', 'agent/credentials', (value) => (credentials = value))
  const loadDelegations = reader('delegations', 'agent/delegations', (value) => (delegations = value))

  function loadTab() {
    if (tab === 'overview') {
      void loadPrincipal()
      if (unlocked) { void loadCredentials(); void loadDelegations() }
    } else if (tab === 'capabilities') void loadGrants()
  }

  async function request(method: string, params: Record<string, unknown> = {}) {
    const turn = generation
    let made = false
    await attempt(async () => {
      const created: any = await call(method, params)
      if (!current(turn)) return
      made = true
      await bridge(() => window.dmsg.openApproval(created.id))
    })
    return made
  }
  const run = (method: string, params: Record<string, unknown> = {}) => attempt(() => call(method, params))

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
        if (lastSession !== undefined) inspectedAgent = null
        lastSession = next
        generation++
        principal = null
        credentials = []; delegations = []; grants = []
        hosted = { agents: [], hosts: [], vocabulary: [], blocked_senders: [] }
        errors = {}; loaded = {}; busy = false; delegate = ''
        void loadHosted()
      }
      loadTab()
    })
  })

  function delegateTo(agent: string) {
    delegate = agent
    tab = 'overview'
  }

  const tabs = $derived([
    { value: 'overview', label: t('tabPrincipal'), icon: 'plan' as const },
    { value: 'ids', label: t('tabAgentIds'), icon: 'agents' as const, count: hosted.agents.length },
    { value: 'capabilities', label: t('tabCapabilities'), icon: 'security' as const },
    { value: 'discourse', label: t('tabDiscourse'), icon: 'members' as const },
    { value: 'mail', label: t('mail'), icon: 'mail' as const },
    { value: 'knowledge', label: t('knowledge'), icon: 'knowledge' as const },
    { value: 'events', label: t('tabEvents'), icon: 'webhook' as const },
    { value: 'audit', label: t('tabAudit'), icon: 'audit' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <nav class="flex shrink-0 gap-1 overflow-x-auto border-b border-line px-8 [scrollbar-width:none]" aria-label={t('agents')}>
    {#each tabs as item (item.value)}
      <button
        class={cn(
          'relative flex h-10 shrink-0 items-center gap-1.5 rounded-t-lg px-2.5 text-[13px] outline-none transition-colors hover:bg-wash focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-forest',
          tab === item.value ? 'bg-pale font-medium text-forest' : 'text-muted'
        )}
        aria-current={tab === item.value ? 'page' : undefined}
        onclick={() => (tab = item.value as Tab)}
        data-testid={`agents-tab-${item.value}`}
      >
        <Icon name={item.icon} />{item.label}
        {#if item.count}<span class="rounded-full bg-wash px-1.5 text-[10.5px] tabular-nums">{item.count}</span>{/if}
        {#if tab === item.value}<span class="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-forest"></span>{/if}
      </button>
    {/each}
  </nav>
  <div class="min-h-0 flex-1">
    {#key sessionKey}
    {#if errors.hosted}<Notice tone="danger">{errors.hosted}</Notice>{/if}
    {#if tab === 'capabilities' && errors.grants}<Notice tone="danger">{errors.grants}</Notice>{/if}
    {#if tab === 'overview'}
      <Page wide><Overview {principal} principalError={errors.principal ?? ''} {loaded} {errors} {credentials} {delegations} {hosted} {unlocked} {busy} {request} {run} onRetry={loadTab} bind:delegate /></Page>
    {:else if tab === 'ids'}
      <Page><AgentIds {hosted} {unlocked} {busy} {run} {attempt} {delegateTo} /></Page>
    {:else if tab === 'capabilities'}
      <Page wide><Capabilities {hosted} {grants} {unlocked} {busy} {run} /></Page>
    {:else if tab === 'discourse'}
      <Page wide><Discourse agents={hosted.agents} active={hosted.active} /></Page>
    {:else if tab === 'mail'}
      <Page wide>
        {#key route}<Mail
          agents={hosted.agents}
          active={inspectedAgent ?? hosted.active}
          selectAgent={(id) => (inspectedAgent = id)}
          blocked={hosted.blocked_senders ?? []}
          {unlocked}
          manage={() => (tab = 'capabilities')}
        />{/key}
      </Page>
    {:else if tab === 'knowledge'}
      <Page wide
        ><Knowledge
          agents={hosted.agents}
          active={inspectedAgent ?? hosted.active}
          selectAgent={(id) => (inspectedAgent = id)}
          hosts={hosted.hosts}
          {unlocked}
          manage={() => (tab = 'capabilities')}
        /></Page
      >
    {:else if tab === 'events'}
      <Page><Events agents={hosted.agents} {unlocked} /></Page>
    {:else}
      <Page><Audit agents={hosted.agents} /></Page>
    {/if}
    {/key}
  </div>
</div>
