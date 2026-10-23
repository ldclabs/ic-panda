<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'

  /**
   * What this account has authorized and is still in force, in one place:
   * the devices that approve formal signatures and web requests, the
   * browsers paired through connect.dmsg.net, web sessions this device
   * vouched for, and what agents may do. Each is managed where it lives;
   * this view only reads.
   */
  let { requests }: { requests: any[] } = $props()

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
    requests.filter((r) => r.type === 'connect' && r.detail?.method === 'authenticate' && r.state === 'done' && (r.detail?.content?.expires_at ?? 0) > Date.now())
  )
  const agentName = (did: string) => hosted.agents.find((a: any) => a.agent_id === did)?.label ?? did
</script>

{#snippet manage(href: string)}
  <a class="inline-flex items-center gap-1 text-[12.5px] text-forest hover:underline" {href}>{t('manage')}<Icon name="chevron-end" class="size-3.5" /></a>
{/snippet}

<Section title={t('formalDevices')} description={t('formalDevicesHint')} icon="device" flush testid="grants-devices">
  {#snippet actions()}{@render manage('#/settings/devices')}{/snippet}
  {#if errors.devices}<Notice tone="danger">{errors.devices}</Notice>{/if}
  <ul class="divide-y divide-line border-t border-line">
    {#each devices as d (d.device_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-[13px]">
        <Id id={d.device_id} />
        {#if d.current}<Pill tone="forest">{t('thisDevice')}</Pill>{/if}
        <span class="ms-auto text-[11.5px] text-faint">{when(d.added_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{loaded.devices ? (errors.devices ? '' : t('noFormalDevices')) : t('loading')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('pairedBrowsers')} description={t('pairedBrowsersHint')} icon="browser" flush testid="grants-browsers">
  {#snippet actions()}{@render manage('#/settings/browsers')}{/snippet}
  {#if errors.browsers}<Notice tone="danger">{errors.browsers}</Notice>{/if}
  <ul class="divide-y divide-line border-t border-line">
    {#each pairings as p (p.pairing_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-[13px]">
        <b class="font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
        <span class="ms-auto text-[11.5px] text-faint">{t('lastUsed')} {when(p.last_used_at)} · {t('expires')} {when(p.expires_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{loaded.browsers ? (errors.browsers ? '' : t('noPairedBrowsers')) : t('loading')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('webSessions')} description={t('webSessionsHint')} icon="website" flush testid="grants-sessions">
  <ul class="divide-y divide-line border-t border-line">
    {#each sessions as r (r.id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-[13px]">
        <b class="font-medium">{r.detail.app_id}</b>
        <bdi dir="ltr" class="font-mono text-[12px] text-muted">{r.detail.origin}</bdi>
        <span class="ms-auto text-[11.5px] text-faint">{t('expires')} {when(r.detail.content.expires_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noWebSessions')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('agentGrants')} description={t('agentGrantsHint')} icon="agents" flush testid="grants-agents">
  {#snippet actions()}{@render manage('#/agents')}{/snippet}
  {#each ['grants', 'credentials', 'agents'] as key}{#if errors[key]}<Notice tone="danger">{errors[key]}</Notice>{/if}{/each}
  <ul class="divide-y divide-line border-t border-line">
    {#each grants as g (g.id)}
      <li class="px-5 py-2.5">
        <div class="flex flex-wrap items-center gap-1.5 text-[13px]">
          <b class="font-medium">{agentName(g.grantee)}</b>
          {#if g.expires_at}<span class="text-[11.5px] text-faint">{t('expires')} {when(g.expires_at)}</span>{/if}
        </div>
        <div class="mt-1 flex flex-wrap gap-1">{#each g.capabilities as capability, i (i)}<Pill class="font-mono font-normal">{capability}</Pill>{/each}</div>
      </li>
    {/each}
    {#each credentials as c (c.id)}
      <li class="px-5 py-2.5">
        <div class="flex flex-wrap items-center gap-1.5 text-[13px]">
          <b class="font-medium">{agentName(c.subject)}</b><Pill tone="forest">{t('credentials')}</Pill>
          {#if c.expires_at}<span class="text-[11.5px] text-faint">{t('expires')} {when(c.expires_at)}</span>{/if}
        </div>
        <div class="mt-1 flex flex-wrap gap-1">{#each c.scopes as scope (scope)}<Pill class="font-mono font-normal">{scope}</Pill>{/each}</div>
      </li>
    {/each}
    {#if !grants.length && !credentials.length}<li class="px-5 py-3 text-[13px] text-faint">{!loaded.grants || !loaded.credentials ? t('loading') : errors.grants || errors.credentials ? '' : t('noAgentGrants')}</li>{/if}
  </ul>
</Section>
