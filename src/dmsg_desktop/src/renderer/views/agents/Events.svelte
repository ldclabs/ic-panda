<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte'
  import CopyButton from '../../components/CopyButton.svelte'
  import Confirm from '../../components/Confirm.svelte'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { iconButtonClass, buttonClass } from '../../components/ui'
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
    if (busy || !alive) return
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
      if (alive && turn === revision) { endpoints = value.endpoints; listLoaded = true; loadErrors.list = '' }
    } catch (e) { if (alive && turn === revision) loadErrors.list = errorText(e) }

  })
  const reload = () => { void readStatus(); void readEndpoints() }

  async function showSecret(endpoint_id: string, value: string) {
    secret = { endpoint_id, secret: value }
    await tick()
    document.getElementById('endpoint-credential')?.focus()
  }

  const create = () =>
    attempt(async () => {
      const submittedLabel = label, submittedSession = session
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
      if (label === submittedLabel) label = ''
      if (session === submittedSession) session = ''
      await showSecret(endpoint.endpoint_id, value)
      if (alive) void readEndpoints()
    })

  /** The old secret stays valid for a day, so the upstream can switch. */
  async function rotate(id: string) {
    const rotated: any = await call('mcp-events/rotate', { endpoint_id: id })
    if (alive) await showSecret(id, rotated.secret)
  }

  async function remove(id: string) {
    await call('mcp-events/delete', { endpoint_id: id })
    if (!alive) return
    revision++
    if (secret?.endpoint_id === id) secret = null
    void readEndpoints()
  }

  onMount(() => {
    void reload()
    return onChange(['mcp-events/changed', 'session/changed'], reload)
  })

  const labelOf = (did?: string | null) => (did ? (agents.find((a) => a.agent_id === did)?.label ?? did) : t('owner'))
</script>

<Section title={t('mcpEvents')} description={t('mcpEventsHint')} icon="webhook" flush testid="agent-events">
  {#if relay}
    <div class="px-5 pb-3">
      <Pill tone={relay.fetching ? 'ok' : 'neutral'} testid="events-status">
        <span class="size-1.5 rounded-full bg-current"></span>{relay.fetching ? t('eventsFetching') : t('eventsPaused')}
      </Pill>
    </div>
  {/if}
  {#each Object.entries(loadErrors).filter(([, message]) => message) as [part, message] (part)}
    <Notice tone="danger" testid={'events-error-' + part}>{message}
      {#snippet action()}<button class={buttonClass('secondary', 'sm')} onclick={part === 'list' ? readEndpoints : readStatus}>{t('retry')}</button>{/snippet}
    </Notice>
  {/each}
  {#if relay?.last_fetch || relay?.rejected || error}
    <div class="grid gap-2 px-5 pb-3">
      {#if relay?.last_fetch}
        <p class="text-callout text-muted">
          {t('lastFetch')} {when(relay.last_fetch.at)}{relay.last_fetch.error ? ` · ${t('error')} ${relay.last_fetch.error}` : ''}{relay.rejected ? ` · ${t('eventsRejected')} ${relay.rejected}` : ''}
        </p>
      {/if}
      {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    </div>
  {/if}
  <ul class="divide-y divide-line" data-testid="event-endpoints">
    {#each endpoints as endpoint (endpoint.endpoint_id)}
      <li class="grid gap-1.5 px-5 py-3">
        <div class="flex flex-wrap items-center gap-2">
          <b class="text-body font-medium">{endpoint.context?.label ?? endpoint.endpoint_id.slice(0, 12)}</b>
          <Pill tone={endpoint.state === 'active' ? 'ok' : 'neutral'}>{endpoint.state}</Pill>
          <span class="text-callout text-muted">{labelOf(endpoint.agent ?? endpoint.context?.agent)} · {when(endpoint.created_at)}</span>
          <span class="flex-1"></span>
          {#if endpoint.state === 'active' && unlocked}
            <button class={buttonClass('subtle', 'xs')} id={'endpoint-rotate-' + endpoint.endpoint_id} disabled={busy} onclick={() => (rotating = endpoint)} data-testid="endpoint-rotate">{t('rotateSecret')}</button>
            <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => (removing = endpoint)} data-testid="endpoint-remove">{t('remove')}</button>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          <bdi dir="ltr" class="min-w-0 truncate font-mono text-code text-muted">{endpoint.url}</bdi>
          <CopyButton class={iconButtonClass('ghost', 'md')} aria-label={t('copy') + ' · ' + endpoint.url} value={() => endpoint.url} />
        </div>
        {#if endpoint.binding?.server}
          <p class="text-callout text-muted">{endpoint.binding.server}{endpoint.binding.refresh_before ? ` · ${t('refreshBefore')} ${when(endpoint.binding.refresh_before)}` : ''}</p>
        {/if}
        {#if endpoint.relay}
          <p class="text-callout text-muted">
            {endpoint.relay.state} · {t('eventsPending')} {endpoint.relay.pending ?? 0}{endpoint.relay.last_delivery_at ? ` · ${when(endpoint.relay.last_delivery_at)}` : ''}
          </p>
        {/if}
        {#if secret?.endpoint_id === endpoint.endpoint_id}
          <Notice tone="warn">
            <p>{t('secretOnce')}</p>
            <pre class="mt-1.5 rounded-lg bg-surface px-2.5 py-2 font-mono text-code break-all whitespace-pre-wrap text-ink select-all" data-testid="endpoint-secret">{secret?.secret}</pre>
            {#snippet action()}
              <button id="endpoint-credential" class={buttonClass('subtle', 'xs')} aria-label={t('copy') + ' · ' + (endpoint.context?.label ?? endpoint.endpoint_id)} onclick={() => secret && copyText(secret.secret)}><Icon name="copy" />{t('copy')}</button>
              <button class={buttonClass('ghost', 'xs')} onclick={async () => { secret = null; await tick(); document.getElementById('endpoint-rotate-' + endpoint.endpoint_id)?.focus() }}>{t('done')}</button>
            {/snippet}
          </Notice>
        {/if}
      </li>
    {:else}
      {#if !loadErrors.list}<li class="px-5 py-3 text-body text-faint"><span role="status">{listLoaded ? t('noEndpoints') : t('loading')}</span></li>{/if}
    {/each}
  </ul>
  {#if unlocked}
    <form onsubmit={(e) => { e.preventDefault(); void create() }} class="grid gap-2 border-t border-line bg-surface-2/60 px-5 py-3.5 sm:grid-cols-[1fr_1fr_1fr_auto] sm:items-end">
      <Field label={t('label')}><input bind:value={label} data-testid="endpoint-label" /></Field>
      <Field label={t('agentIds')}>
        <Select bind:value={agent} label={t('tabAgentIds')} class="w-full" options={[{ value: '', label: t('anyAgent') }, ...agents.map((a) => ({ value: a.agent_id, label: a.label }))]} />
      </Field>
      <Field label={t('eventsSession')}><input bind:value={session} /></Field>
      <button class={buttonClass('primary')} disabled={busy} data-testid="endpoint-create" type="submit">{t('create')}</button>
    </form>
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
  onConfirm={() => rotate(rotating.endpoint_id)}
  onReview={readEndpoints}
/>

<Confirm
  bind:open={() => !!removing, (open) => !open && (removing = null)}
  title={t('removeEndpointTitle', { label: removing?.context?.label ?? removing?.endpoint_id?.slice(0, 12) ?? '' })}
  description={t('removeEndpointHint')}
  confirm={t('remove')}
  {busy}
  testid="endpoint-remove-dialog"
  onConfirm={() => remove(removing.endpoint_id)}
  onReview={readEndpoints}
/>
