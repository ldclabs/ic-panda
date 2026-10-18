<script lang="ts">
  import { onMount } from 'svelte'
  import Locked from '../components/page/Locked.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { toast } from '../components/toast.svelte'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, closed, errorText, onChange, single } from '../rpc'
  import Bundles from './approvals/Bundles.svelte'
  import Grants from './approvals/Grants.svelte'
  import Requests from './approvals/Requests.svelte'
  import Sign from './approvals/Sign.svelte'

  /**
   * Signatures and approvals in parts: the requests waiting, running and
   * decided; signing from this device with what it needs first; evidence,
   * verified and exported; and what the account has authorized. Every
   * decision is the approval window's.
   */
  let { status }: { status: any } = $props()

  type Tab = 'requests' | 'sign' | 'bundles' | 'grants'

  let tab = $state<Tab>('requests')
  let requests = $state<any[]>([])
  let history = $state<any[]>([])
  let agents = $state<Record<string, string>>({})
  let error = $state('')
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const waiting = $derived(requests.filter((r) => r.state === 'awaiting_user'))
  const known = $derived(new Set(requests.map((r) => r.id)))

  const reload = single(async () => {
    error = ''
    if (!unlocked) {
      requests = []
      history = []
      return
    }
    try {
      ;[requests, history] = await Promise.all([call('approval/list', { all: true }), call('attest/history')])
      if (requests.some((r) => String(r.source).startsWith('agent:'))) {
        const hosted: any = await call('agent/ids').catch(() => null)
        agents = Object.fromEntries((hosted?.agents ?? []).map((a: any) => [a.agent_id, a.label]))
      }
    } catch (e) {
      requests = []
      history = []
      if (!closed(e, status)) error = errorText(e)
    }
  })

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }

  const open = (id: string) => attempt(() => bridge(() => window.dmsg.openApproval(id)))
  /** Withdraw a request this device's own clients made; a web request is the page's to cancel. */
  const cancel = (id: string) => attempt(() => call('approval/cancel', { id }))
  /** Settle approved requests whose outcome is not known yet, now rather than at the next pass. */
  const reconcile = () => attempt(() => call('approval/reconcile'))

  onMount(() => onChange(['approval/changed', 'session/changed'], reload))

  // Read on opening, and again when the device locks or unlocks.
  $effect(() => {
    void unlocked
    void reload()
  })

  const tabs = $derived([
    { value: 'requests', label: t('requests'), icon: 'approvals' as const, count: waiting.length },
    { value: 'sign', label: t('newSignature'), icon: 'signature' as const },
    { value: 'bundles', label: t('bundles'), icon: 'verified' as const },
    { value: 'grants', label: t('authorizations'), icon: 'security' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, (next) => (tab = next as Tab)} items={tabs} label={t('approvals')} testid="approvals-tab" />
  <div class="flex min-h-0 flex-1 flex-col">
    {#if !unlocked && tab !== 'bundles'}
      <Locked {status} testid="approvals-locked" />
    {:else}
      <Page>
        {#if error}<Notice tone="danger" testid="approvals-error">{error}</Notice>{/if}
        {#if tab === 'requests'}
          <Requests {requests} {agents} {busy} {open} {cancel} {reconcile} />
        {:else if tab === 'sign'}
          <Sign {open} />
        {:else if tab === 'bundles'}
          <Bundles {history} {known} {unlocked} {open} />
        {:else}
          <Grants {requests} />
        {/if}
      </Page>
    {/if}
  </div>
</div>
