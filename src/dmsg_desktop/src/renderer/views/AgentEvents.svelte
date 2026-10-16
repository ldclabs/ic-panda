<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Select from '../components/Select.svelte'
  import { buttonClass } from '../components/ui'

  /**
   * MCP Events endpoints on the relay: an upstream subscription posts to the
   * URL with the secret, shown once; agents read the deliveries. This view
   * manages endpoints and never reads or acknowledges deliveries.
   */
  let { agents }: { agents: any[] } = $props()
  let endpoints = $state<any[]>([])
  let relay = $state<any>(null)
  let label = $state('')
  let agent = $state('')
  let session = $state('')
  let secret = $state<{ endpoint_id: string; secret: string; url?: string } | null>(null)
  let error = $state('')
  let busy = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  const load = () =>
    attempt(async () => {
      relay = await call('mcp-events/status')
      endpoints = (await call('mcp-events/list')).endpoints
    })

  const create = () =>
    attempt(async () => {
      const context: Record<string, string> = {}
      if (label.trim()) context.label = label.trim()
      if (agent) context.agent = agent
      if (session.trim()) context.session = session.trim()
      const created: any = await call('mcp-events/create', { context })
      secret = { endpoint_id: created.endpoint_id, secret: created.secret, url: created.url }
      label = ''
      session = ''
      endpoints = (await call('mcp-events/list')).endpoints
    })

  /** The old secret stays valid for a day, so the upstream can switch. */
  const rotate = (id: string) =>
    attempt(async () => {
      const rotated: any = await call('mcp-events/rotate', { endpoint_id: id })
      secret = { endpoint_id: id, secret: rotated.secret }
    })

  const remove = (id: string) =>
    attempt(async () => {
      await call('mcp-events/delete', { endpoint_id: id })
      if (secret?.endpoint_id === id) secret = null
      endpoints = (await call('mcp-events/list')).endpoints
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['mcp-events/changed', 'session/changed'].includes(event.type)) void load()
    })
  })

  const labelOf = (did?: string | null) => (did ? (agents.find((a) => a.agent_id === did)?.label ?? did) : t('owner'))
</script>

<div class="card" data-testid="agent-events">
  <h2>{t('mcpEvents')}</h2>
  <p class="muted">{t('mcpEventsHint')}</p>
  {#if error}<p class="error">{error}</p>{/if}
  {#if relay}
    <p class="muted" data-testid="events-status">
      {relay.fetching ? t('eventsFetching') : t('eventsPaused')}{relay.last_fetch?.at ? ` · ${t('lastFetch')} ${when(relay.last_fetch.at)}` : ''}{relay
        .last_fetch?.error
        ? ` · ${t('error')} ${relay.last_fetch.error}`
        : ''}{relay.rejected ? ` · ${t('eventsRejected')} ${relay.rejected}` : ''}
    </p>
  {/if}
  <ul class="list" data-testid="event-endpoints">
    {#each endpoints as endpoint (endpoint.endpoint_id)}
      <li>
        <div class="row spread">
          <span>
            <strong>{endpoint.context?.label ?? endpoint.endpoint_id.slice(0, 12)}</strong>
            <span class="badge">{endpoint.state}</span>
            <span class="muted">· {labelOf(endpoint.agent ?? endpoint.context?.agent)} · {when(endpoint.created_at)}</span>
          </span>
          {#if endpoint.state === 'active'}
            <span class="row">
              <button class={buttonClass()} disabled={busy} onclick={() => rotate(endpoint.endpoint_id)}>{t('rotateSecret')}</button>
              <button class={buttonClass('danger')} disabled={busy} onclick={() => remove(endpoint.endpoint_id)}>{t('remove')}</button>
            </span>
          {/if}
        </div>
        <div class="mono muted">{endpoint.url}</div>
        {#if endpoint.binding?.server}
          <div class="muted">
            {endpoint.binding.server}{endpoint.binding.refresh_before ? ` · ${t('refreshBefore')} ${when(endpoint.binding.refresh_before)}` : ''}
          </div>
        {/if}
        {#if endpoint.relay}
          <div class="muted">{endpoint.relay.state} · {t('eventsPending')} {endpoint.relay.pending ?? 0}{endpoint.relay.last_delivery_at ? ` · ${when(endpoint.relay.last_delivery_at)}` : ''}</div>
        {/if}
        {#if secret?.endpoint_id === endpoint.endpoint_id}
          <p class="warn">{t('secretOnce')}</p>
          <pre class="mono" data-testid="endpoint-secret">{secret?.secret}</pre>
        {/if}
      </li>
    {:else}
      <li class="muted">{t('noEndpoints')}</li>
    {/each}
  </ul>
  <div class="row" style="margin-top: 8px">
    <input placeholder={t('label')} bind:value={label} data-testid="endpoint-label" style="max-width: 180px" />
    <Select bind:value={agent} options={[{ value: '', label: t('anyAgent') }, ...agents.map((a) => ({ value: a.agent_id, label: a.label }))]} />
    <input placeholder={t('eventsSession')} bind:value={session} style="max-width: 180px" />
    <button class={buttonClass('primary')} disabled={busy} data-testid="endpoint-create" onclick={create}>{t('create')}</button>
  </div>
</div>

<style>
  pre {
    margin: 0;
    white-space: pre-wrap;
  }
</style>
