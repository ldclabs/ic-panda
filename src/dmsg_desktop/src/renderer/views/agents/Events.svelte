<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Confirm from '../../components/Confirm.svelte'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'

  /**
   * MCP Events endpoints on the relay: an upstream subscription posts to the
   * URL with the secret, shown once; agents read the deliveries. This view
   * manages endpoints and never reads or acknowledges deliveries; rotating
   * a secret or deleting an endpoint asks first.
   */
  let { agents, unlocked }: { agents: any[]; unlocked: boolean } = $props()

  let alive = true
  onDestroy(() => { alive = false })
  let endpoints = $state<any[]>([])
  let listLoaded = $state(false)
  let revision = 0
  let relay = $state<any>(null)
  let label = $state('')
  let agent = $state('')
  let session = $state('')
  let secret = $state<{ endpoint_id: string; secret: string } | null>(null)
  let rotating = $state<any>(null)
  let removing = $state<any>(null)
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  let loadErrors = $state<Record<string, string>>({})
  const readStatus = single(async () => {
    if (!alive) return
    try {
      const value = await call('mcp-events/status')
      if (alive) { relay = value; loadErrors.status = '' }
    } catch (e) { if (alive) loadErrors.status = errorText(e) }
  })
  const readEndpoints = single(async () => {
    if (!alive) return
    const turn = revision
    try {
      const value = await call('mcp-events/list')
      if (alive && turn === revision) { endpoints = value.endpoints; loadErrors.list = '' }
    } catch (e) { if (alive && turn === revision) loadErrors.list = errorText(e) }
    finally { if (alive && turn === revision) listLoaded = true }
  })
  const reload = () => { void readStatus(); void readEndpoints() }

  const create = () =>
    attempt(async () => {
      const context: Record<string, string> = {}
      if (label.trim()) context.label = label.trim()
      if (agent) context.agent = agent
      if (session.trim()) context.session = session.trim()
      const created: any = await call('mcp-events/create', { context })
      if (!alive) return
      const { secret: value, ...endpoint } = created
      revision++
      endpoints = [...endpoints.filter((e) => e.endpoint_id !== endpoint.endpoint_id), endpoint]
      listLoaded = true
      secret = { endpoint_id: endpoint.endpoint_id, secret: value }
      label = ''
      session = ''
      if (alive) void readEndpoints()
    })

  /** The old secret stays valid for a day, so the upstream can switch. */
  const rotate = (id: string) =>
    attempt(async () => {
      const rotated: any = await call('mcp-events/rotate', { endpoint_id: id })
      if (!alive) return
      secret = { endpoint_id: id, secret: rotated.secret }
    })

  const remove = (id: string) =>
    attempt(async () => {
      await call('mcp-events/delete', { endpoint_id: id })
      if (!alive) return
      revision++
      if (secret?.endpoint_id === id) secret = null
      if (alive) void readEndpoints()
    })

  onMount(() => {
    void reload()
    return onChange(['mcp-events/changed', 'session/changed'], reload)
  })

  const labelOf = (did?: string | null) => (did ? (agents.find((a) => a.agent_id === did)?.label ?? did) : t('owner'))
</script>

<Section title={t('mcpEvents')} description={t('mcpEventsHint')} icon="webhook" flush testid="agent-events">
  {#snippet actions()}
    {#if relay}
      <Pill tone={relay.fetching ? 'ok' : 'neutral'} testid="events-status">
        <span class="size-1.5 rounded-full bg-current"></span>{relay.fetching ? t('eventsFetching') : t('eventsPaused')}
      </Pill>
    {/if}
  {/snippet}
  {#each Object.values(loadErrors).filter(Boolean) as failure}<Notice tone="danger">{failure}</Notice>{/each}
  {#if relay?.last_fetch || relay?.rejected || error}
    <div class="grid gap-2 px-5 pb-3">
      {#if relay?.last_fetch}
        <p class="text-[12px] text-faint">
          {t('lastFetch')} {when(relay.last_fetch.at)}{relay.last_fetch.error ? ` · ${t('error')} ${relay.last_fetch.error}` : ''}{relay.rejected ? ` · ${t('eventsRejected')} ${relay.rejected}` : ''}
        </p>
      {/if}
      {#if error}<Notice tone="danger">{error}</Notice>{/if}
    </div>
  {/if}
  <ul class="divide-y divide-line border-t border-line" data-testid="event-endpoints">
    {#each endpoints as endpoint (endpoint.endpoint_id)}
      <li class="grid gap-1.5 px-5 py-3">
        <div class="flex flex-wrap items-center gap-2">
          <b class="text-[13px] font-medium">{endpoint.context?.label ?? endpoint.endpoint_id.slice(0, 12)}</b>
          <Pill tone={endpoint.state === 'active' ? 'ok' : 'neutral'}>{endpoint.state}</Pill>
          <span class="text-[12px] text-faint">{labelOf(endpoint.agent ?? endpoint.context?.agent)} · {when(endpoint.created_at)}</span>
          <span class="flex-1"></span>
          {#if endpoint.state === 'active' && unlocked}
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => (rotating = endpoint)} data-testid="endpoint-rotate">{t('rotateSecret')}</button>
            <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => (removing = endpoint)} data-testid="endpoint-remove">{t('remove')}</button>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          <bdi dir="ltr" class="min-w-0 truncate font-mono text-[11.5px] text-muted">{endpoint.url}</bdi>
          <button class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('copy')} onclick={() => copyText(endpoint.url)}><Icon name="copy" /></button>
        </div>
        {#if endpoint.binding?.server}
          <p class="text-[12px] text-faint">{endpoint.binding.server}{endpoint.binding.refresh_before ? ` · ${t('refreshBefore')} ${when(endpoint.binding.refresh_before)}` : ''}</p>
        {/if}
        {#if endpoint.relay}
          <p class="text-[12px] text-faint">
            {endpoint.relay.state} · {t('eventsPending')} {endpoint.relay.pending ?? 0}{endpoint.relay.last_delivery_at ? ` · ${when(endpoint.relay.last_delivery_at)}` : ''}
          </p>
        {/if}
        {#if secret?.endpoint_id === endpoint.endpoint_id}
          <Notice tone="warn">
            <p>{t('secretOnce')}</p>
            <pre class="mt-1.5 rounded-lg bg-surface px-2.5 py-2 font-mono text-[11.5px] break-all whitespace-pre-wrap text-ink select-all" data-testid="endpoint-secret">{secret?.secret}</pre>
            {#snippet action()}
              <button class={buttonClass('secondary', 'xs')} onclick={() => secret && copyText(secret.secret)}><Icon name="copy" />{t('copy')}</button>
            {/snippet}
          </Notice>
        {/if}
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">{listLoaded ? t('noEndpoints') : t('loading')}</li>
    {/each}
  </ul>
  {#if unlocked}
    <div class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5 sm:grid-cols-[1fr_1fr_1fr_auto] sm:items-end">
      <Field label={t('label')}><input bind:value={label} data-testid="endpoint-label" /></Field>
      <Field label={t('agentIds')}>
        <Select bind:value={agent} class="w-full" options={[{ value: '', label: t('anyAgent') }, ...agents.map((a) => ({ value: a.agent_id, label: a.label }))]} />
      </Field>
      <Field label={t('eventsSession')}><input bind:value={session} /></Field>
      <button class={buttonClass('primary')} disabled={busy} data-testid="endpoint-create" onclick={create}>{t('create')}</button>
    </div>
  {/if}
</Section>

<Confirm
  bind:open={() => !!rotating, (open) => !open && (rotating = null)}
  title={t('rotateSecretTitle', { label: rotating?.context?.label ?? rotating?.endpoint_id?.slice(0, 12) ?? '' })}
  description={t('rotateSecretHint')}
  confirm={t('rotateSecret')}
  tone="warn"
  {busy}
  testid="endpoint-rotate-dialog"
  onConfirm={() => rotating && rotate(rotating.endpoint_id)}
/>

<Confirm
  bind:open={() => !!removing, (open) => !open && (removing = null)}
  title={t('removeEndpointTitle', { label: removing?.context?.label ?? removing?.endpoint_id?.slice(0, 12) ?? '' })}
  description={t('removeEndpointHint')}
  confirm={t('remove')}
  {busy}
  testid="endpoint-remove-dialog"
  onConfirm={() => removing && remove(removing.endpoint_id)}
/>
