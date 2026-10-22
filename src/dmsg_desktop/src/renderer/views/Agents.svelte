<script lang="ts">
  import { onMount } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import Icon from '../components/Icon.svelte'
  import { cn } from '../components/ui'
  import { toast } from '../components/toast.svelte'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, single } from '../rpc'
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
  let { status }: { status: any } = $props()

  type Tab = 'overview' | 'ids' | 'capabilities' | 'discourse' | 'mail' | 'knowledge' | 'events' | 'audit'

  let tab = $state<Tab>('overview')
  let inspectedAgent = $state<string | null>(null)
  let principal = $state<any>(null)
  let principalError = $state('')
  let credentials = $state<any[]>([])
  let delegations = $state<any[]>([])
  let hosted = $state<any>({ agents: [], hosts: [], vocabulary: [], blocked_senders: [] })
  let grants = $state<any[]>([])
  /** An Agent ID the principal tab grants a credential to next. */
  let delegate = $state('')
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')

  $effect(() => {
    if (inspectedAgent && !hosted.agents.some((agent: any) => agent.agent_id === inspectedAgent)) inspectedAgent = null
  })

  const fail = (e: unknown) => toast(() => errorText(e))

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      fail(e)
    } finally {
      busy = false
    }
  }

  /** What only a lock or another account hides is no error. */
  const quiet = (e: unknown) => ['LOCKED', 'AUTH_REQUIRED', 'RECOVERY_INCOMPLETE'].includes(code(e))

  const loadHosted = single(async () => {
    try {
      hosted = await call('agent/ids')
    } catch (e) {
      if (!quiet(e)) fail(e)
    }
  })

  const loadGrants = single(async () => {
    grants = await call<any[]>('grants/list').catch(() => [])
  })

  /** The principal, then what the service and this device keep of its credentials; an error stays on the card. */
  const loadPrincipal = single(async () => {
    try {
      principal = await call('agent/principal')
      principalError = ''
    } catch (e) {
      principalError = quiet(e) ? '' : errorText(e)
      return
    }
    delegations = await call<any[]>('agent/delegations').catch(() => [])
    try {
      credentials = principal ? await call<any[]>('agent/credentials') : []
    } catch (e) {
      principalError = quiet(e) ? '' : errorText(e)
    }
  })

  const reload = () => Promise.all([loadHosted(), loadGrants(), loadPrincipal()])

  /** Principal changes and grants are approval requests the approval window decides; whether one was made. */
  async function request(method: string, params: Record<string, unknown> = {}) {
    let made = false
    await attempt(async () => {
      const created: any = await call(method, params)
      made = true
      await bridge(() => window.dmsg.openApproval(created.id))
    })
    return made
  }

  const run = (method: string, params: Record<string, unknown> = {}) => attempt(() => call(method, params))

  onMount(() => {
    void reload()
    const offs = [
      onChange(['agent/changed', 'session/changed'], reload),
      // A decided request may change the principal, its credentials or the grants.
      onChange(['approval/changed'], () => Promise.all([loadPrincipal(), loadGrants()]))
    ]
    return () => offs.forEach((off) => off())
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
  const groups = $derived([
    { label: t('agentNavIdentity'), items: tabs.slice(0, 3) },
    { label: t('agentNavActivity'), items: tabs.slice(3, 6) },
    { label: t('agentNavOperations'), items: tabs.slice(6) }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <nav class="flex shrink-0 gap-5 overflow-x-auto border-b border-line px-8 pt-3 [scrollbar-width:none]" aria-label={t('agents')}>
    {#each groups as group}
      <div class="shrink-0">
        <p class="px-2 pb-1 text-[10.5px] font-medium text-faint">{group.label}</p>
        <div class="flex gap-1">
          {#each group.items as item (item.value)}
            <button
              class={cn(
                'relative flex h-10 items-center gap-1.5 rounded-t-lg px-2.5 text-[13px] outline-none transition-colors hover:bg-wash focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-forest',
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
        </div>
      </div>
    {/each}
  </nav>
  <div class="min-h-0 flex-1">
    {#if tab === 'overview'}
      <Page wide><Overview {principal} {principalError} {credentials} {delegations} {hosted} {unlocked} {busy} {request} {run} bind:delegate /></Page>
    {:else if tab === 'ids'}
      <Page><AgentIds {hosted} {unlocked} {busy} {run} {attempt} {delegateTo} /></Page>
    {:else if tab === 'capabilities'}
      <Page wide><Capabilities {hosted} {grants} {unlocked} {busy} {run} /></Page>
    {:else if tab === 'discourse'}
      <Page wide><Discourse agents={hosted.agents} active={hosted.active} /></Page>
    {:else if tab === 'mail'}
      <Page wide
        ><Mail
          agents={hosted.agents}
          active={inspectedAgent ?? hosted.active}
          selectAgent={(id) => (inspectedAgent = id)}
          blocked={hosted.blocked_senders ?? []}
          {unlocked}
          manage={() => (tab = 'capabilities')}
        /></Page
      >
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
  </div>
</div>
