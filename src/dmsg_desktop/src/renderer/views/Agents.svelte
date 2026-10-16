<script lang="ts">
  import { onMount } from 'svelte'
  import Page from '../components/page/Page.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { toast } from '../components/toast.svelte'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code } from '../rpc'
  import AgentIds from './agents/AgentIds.svelte'
  import Audit from './agents/Audit.svelte'
  import Capabilities from './agents/Capabilities.svelte'
  import Events from './agents/Events.svelte'
  import Mail from './agents/Mail.svelte'
  import Overview from './agents/Overview.svelte'

  /**
   * Agents in parts: the account's principal with its controllers and
   * credentials, the hosted Agent IDs, what each may reach, Agent Mail and
   * Knowledge, MCP Events endpoints, and the audit record.
   */
  let { status }: { status: any } = $props()

  type Tab = 'overview' | 'ids' | 'capabilities' | 'mail' | 'events' | 'audit'

  let tab = $state<Tab>('overview')
  let principal = $state<any>(null)
  let credentials = $state<any[]>([])
  let hosted = $state<any>({ agents: [], hosts: [] })
  let grants = $state<any[]>([])
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')

  const fail = (e: unknown) => toast(() => (code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`))

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

  async function load() {
    try {
      hosted = await call('agent/ids')
      principal = await call('agent/principal')
      grants = await call('grants/list')
      credentials = principal ? await call('agent/credentials') : []
    } catch (e) {
      if (code(e) !== 'LOCKED') fail(e)
    }
  }

  /** Principal changes and grants are approval requests the approval window decides. */
  const request = (method: string, params: Record<string, unknown> = {}) =>
    attempt(async () => {
      const created: any = await call(method, params)
      await bridge(() => window.dmsg.openApproval(created.id))
    })

  const run = (method: string, params: Record<string, unknown> = {}) =>
    attempt(async () => {
      await call(method, params)
      await load()
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed', 'agent/changed'].includes(event.type)) void load()
    })
  })

  const tabs = $derived([
    { value: 'overview', label: t('tabPrincipal'), icon: 'plan' as const },
    { value: 'ids', label: t('tabAgentIds'), icon: 'agents' as const, count: hosted.agents.length },
    { value: 'capabilities', label: t('tabCapabilities'), icon: 'security' as const },
    { value: 'mail', label: t('mail'), icon: 'mail' as const },
    { value: 'events', label: t('tabEvents'), icon: 'webhook' as const },
    { value: 'audit', label: t('tabAudit'), icon: 'audit' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, (next) => (tab = next as Tab)} items={tabs} label={t('agents')} testid="agents-tab" />
  <div class="min-h-0 flex-1">
    {#if tab === 'overview'}
      <Page><Overview {principal} {credentials} {hosted} {unlocked} {busy} {request} {run} /></Page>
    {:else if tab === 'ids'}
      <Page><AgentIds {hosted} {unlocked} {busy} {run} {attempt} reload={load} /></Page>
    {:else if tab === 'capabilities'}
      <Page wide><Capabilities {hosted} {grants} {unlocked} {busy} {run} /></Page>
    {:else if tab === 'mail'}
      <Page><Mail agents={hosted.agents} active={hosted.active} {unlocked} /></Page>
    {:else if tab === 'events'}
      <Page><Events agents={hosted.agents} {unlocked} /></Page>
    {:else}
      <Page><Audit agents={hosted.agents} /></Page>
    {/if}
  </div>
</div>
