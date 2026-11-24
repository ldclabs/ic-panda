<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { live } from '../agents/agents'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'

  /**
   * What this account has authorized and is still in force, in one place:
   * the devices that approve formal signatures and web requests, the
   * browsers paired through connect.dmsg.net, web sessions this device
   * vouched for, and what agents may do. Each is managed where it lives;
   * this view only reads.
   */
  let { requests, loaded: requestsLoaded, error, retry }: { requests: any[]; loaded: boolean; error: string; retry: () => unknown } = $props()
  let now = $state(Date.now())

  let devices = $state<any[]>([])
  let pairings = $state<any[]>([])
  let grants = $state<any[]>([])
  let credentials = $state<any[]>([])
  let hosted = $state<any>({ agents: [] })

  let alive = true
  let loaded = $state<Record<string, boolean>>({})
  let errors = $state<Record<string, string>>({})

  async function read<T>(key: string, fetch: () => Promise<T>, apply: (value: T) => void) {
    if (!alive) return
    try {
      const value = await fetch()
      if (alive) { apply(value); errors[key] = '' }
    } catch (e) {
      if (alive) errors[key] = errorText(e)
    } finally {
      if (alive) loaded[key] = true
    }
  }
  const readDevices = single(() => read('devices', () => call('account/show'), (account: any) => {
    devices = (account?.devices ?? []).filter((d: any) => !d.revoked_at && d.capabilities?.includes('FormalApprove'))
  }))
  const readBrowsers = single(() => read('browsers', () => call('connect/pairings'), (browsers: any[]) => {
    pairings = browsers.filter((p) => p.state === 'active')
  }))
  const readGrants = single(() => read('grants', () => call('grants/list'), (next: any[]) => {
    grants = next.filter((g) => !g.revoked_at)
  }))
  const readAgents = single(() => read('agents', () => call('agent/ids'), (next: any) => (hosted = next)))
  // The credential endpoint returns an empty list when none exist. It does
  // not need a separate principal query before listing them.
  const readCredentials = single(() => read('credentials', () => call('agent/credentials'), (next: any[]) => {
    credentials = next.filter((c) => c.status === 'active')
  }))
  function reload() {
    void readDevices(); void readBrowsers(); void readGrants(); void readAgents(); void readCredentials()
  }

  onMount(() => {
    reload()
    const off = onChange(['approval/changed', 'session/changed', 'agent/changed', 'connect/changed'], reload)
    return () => { alive = false; off() }
  })

  /** Web sessions this device vouched for that have not expired. */
  const sessions = $derived(
    requests.filter((r) => r.type === 'connect' && r.detail?.method === 'authenticate' && r.state === 'done' && (r.detail?.content?.expires_at ?? 0) > now)
  )
  const activeGrants = $derived(grants.filter(g => live(g, now)))
  const activeBrowsers = $derived(pairings.filter(p => !p.expires_at || p.expires_at > now))
  const activeCredentials = $derived(credentials.filter(c => !c.expires_at || c.expires_at > now))
  $effect(() => {
    const expiries = [...activeGrants, ...activeBrowsers, ...activeCredentials].map(row => row.expires_at)
    expiries.push(...sessions.map(row => row.detail.content.expires_at))
    const next = Math.min(...expiries.filter(time => time > now))
    if (!Number.isFinite(next)) return
    const timer = setTimeout(() => (now = Date.now()), Math.min(2_147_483_647, Math.max(1, next - Date.now())))
    return () => clearTimeout(timer)
  })
  const agentName = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did
</script>

{#snippet readFailure(message: string, again: () => unknown)}
  <Notice tone="danger">{message}
    {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={again}>{t('retry')}</button>{/snippet}
  </Notice>
{/snippet}

{#snippet manage(href: string, section: string, label = t('manage'))}
  <a class="inline-flex items-center gap-1 text-callout text-forest hover:underline" {href} aria-label={t('manage') + ' · ' + section}>{label}<Icon name="chevron-end" class="size-3.5" /></a>
{/snippet}

<Section title={t('formalDevices')} description={t('formalDevicesHint')} icon="device" flush testid="grants-devices">
  {#snippet actions()}{@render manage('#/settings/devices', t('formalDevices'))}{/snippet}
  {#if errors.devices}{@render readFailure(errors.devices, readDevices)}{/if}
  <ul class="divide-y divide-line">
    {#each devices as d (d.device_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-body">
        <Id id={d.device_id} />
        {#if d.current}<Pill tone="forest">{t('thisDevice')}</Pill>{/if}
        <span class="ms-auto text-caption text-faint">{when(d.added_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-body text-faint">{loaded.devices ? (errors.devices ? '' : t('noFormalDevices')) : t('loading')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('pairedBrowsers')} description={t('pairedBrowsersHint')} icon="browser" flush testid="grants-browsers">
  {#snippet actions()}{@render manage('#/settings/browsers', t('pairedBrowsers'))}{/snippet}
  {#if errors.browsers}{@render readFailure(errors.browsers, readBrowsers)}{/if}
  <ul class="divide-y divide-line">
    {#each activeBrowsers as p (p.pairing_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-body">
        <b class="font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
        <span class="ms-auto text-caption text-faint">{t('lastUsed')} {when(p.last_used_at)} · {t('expires')} {when(p.expires_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-body text-faint">{loaded.browsers ? (errors.browsers ? '' : t('noPairedBrowsers')) : t('loading')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('webSessions')} description={t('webSessionsHint')} icon="website" flush testid="grants-sessions">
  {#if error}{@render readFailure(error, retry)}{/if}
  <ul class="divide-y divide-line">
    {#each sessions as r (r.id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-body">
        <b class="font-medium">{r.detail.app_id}</b>
        <bdi dir="ltr" class="font-mono text-code text-muted">{r.detail.origin}</bdi>
        <span class="ms-auto text-caption text-faint">{t('expires')} {when(r.detail.content.expires_at)}</span>
      </li>
    {:else}
      {#if !error}<li class="px-5 py-3 text-body text-faint">{requestsLoaded ? t('noWebSessions') : t('loading')}</li>{/if}
    {/each}
  </ul>
</Section>

<Section title={t('agentGrants')} description={t('agentGrantsHint')} icon="agents" flush testid="grants-agents">
  {#snippet actions()}
    {#if activeGrants.length && activeCredentials.length}
      {@render manage('#/agents/capabilities', t('tabCapabilities'), t('tabCapabilities'))}
      {@render manage('#/agents/overview', t('credentials'), t('credentials'))}
    {:else}
      {@render manage(activeCredentials.length ? '#/agents/overview' : '#/agents/capabilities', t('agentGrants'))}
    {/if}
  {/snippet}
  {#if errors.grants}{@render readFailure(errors.grants, readGrants)}{/if}
  {#if errors.credentials}{@render readFailure(errors.credentials, readCredentials)}{/if}
  {#if errors.agents}{@render readFailure(errors.agents, readAgents)}{/if}
  <ul class="divide-y divide-line">
    {#each activeGrants as g (g.id)}
      <li class="px-5 py-2.5">
        <div class="flex flex-wrap items-center gap-1.5 text-body">
          <b class="font-medium">{agentName(g.grantee)}</b>
          {#if g.expires_at}<span class="text-caption text-faint">{t('expires')} {when(g.expires_at)}</span>{/if}
        </div>
        <div class="mt-1 flex flex-wrap gap-1">{#each g.capabilities as capability, i (i)}<Pill class="font-mono font-normal">{capability}</Pill>{/each}</div>
      </li>
    {/each}
    {#each activeCredentials as c (c.id)}
      <li class="px-5 py-2.5">
        <div class="flex flex-wrap items-center gap-1.5 text-body">
          <b class="font-medium">{agentName(c.subject)}</b><Pill tone="forest">{t('credentials')}</Pill>
          {#if c.expires_at}<span class="text-caption text-faint">{t('expires')} {when(c.expires_at)}</span>{/if}
        </div>
        <div class="mt-1 flex flex-wrap gap-1">{#each c.scopes as scope (scope)}<Pill class="font-mono font-normal">{scope}</Pill>{/each}</div>
      </li>
    {/each}
    {#if !activeGrants.length && !activeCredentials.length}<li class="px-5 py-3 text-body text-faint">{!loaded.grants || !loaded.credentials ? t('loading') : errors.grants || errors.credentials ? '' : t('noAgentGrants')}</li>{/if}
  </ul>
</Section>
