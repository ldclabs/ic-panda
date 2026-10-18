<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { t } from '../../i18n/index.svelte'
  import { call, onChange, single, when } from '../../rpc'

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

  const reload = single(async () => {
    const [account, browsers, agentGrants, ids, principal] = await Promise.all([
      call('account/show').catch(() => null),
      call('connect/pairings').catch(() => []),
      call('grants/list').catch(() => []),
      call('agent/ids').catch(() => ({ agents: [] })),
      call('agent/principal').catch(() => null)
    ])
    devices = (account?.devices ?? []).filter((d: any) => !d.revoked_at && d.capabilities?.includes('FormalApprove'))
    pairings = (browsers ?? []).filter((p: any) => p.state === 'active')
    grants = (agentGrants ?? []).filter((g: any) => !g.revoked_at)
    hosted = ids
    credentials = principal ? ((await call('agent/credentials').catch(() => [])) as any[]).filter((c) => c.status === 'active') : []
  })

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed', 'agent/changed', 'connect/changed'], reload)
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
  <ul class="divide-y divide-line border-t border-line">
    {#each devices as d (d.device_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-[13px]">
        <Id id={d.device_id} />
        {#if d.current}<Pill tone="forest">{t('thisDevice')}</Pill>{/if}
        <span class="ms-auto text-[11.5px] text-faint">{when(d.added_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noFormalDevices')}</li>
    {/each}
  </ul>
</Section>

<Section title={t('pairedBrowsers')} description={t('pairedBrowsersHint')} icon="browser" flush testid="grants-browsers">
  {#snippet actions()}{@render manage('#/settings/browsers')}{/snippet}
  <ul class="divide-y divide-line border-t border-line">
    {#each pairings as p (p.pairing_id)}
      <li class="flex items-center gap-2 px-5 py-2.5 text-[13px]">
        <b class="font-medium">{p.label ?? p.pairing_id.slice(0, 12)}</b>
        <span class="ms-auto text-[11.5px] text-faint">{t('lastUsed')} {when(p.last_used_at)} · {t('expires')} {when(p.expires_at)}</span>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{t('noPairedBrowsers')}</li>
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
    {#if !grants.length && !credentials.length}<li class="px-5 py-3 text-[13px] text-faint">{t('noAgentGrants')}</li>{/if}
  </ul>
</Section>
