<script lang="ts">
  import { recoverMailboxes } from '../../../shared/mail-recovery'
  import { onDestroy, onMount, untrack } from 'svelte'
  import Confirm from '../../components/Confirm.svelte'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { ago, call, CallError, code, errorText, single, when } from '../../rpc'

  /** Owner inspection never consumes the Agent's unread queue. */
  let {
    agents,
    active,
    blocked,
    unlocked,
    manage,
    selectAgent
  }: {
    agents: any[]
    active: string | null
    blocked: string[]
    unlocked: boolean
    manage: () => void
    selectAgent: (id: string) => void
  } = $props()

  let alive = true
  let previousAgent = ''
  let operation = 0
  let syncing = $state(false)
  const receiving = new Set<string>()
  onDestroy(() => { alive = false; operation++ })
  const PAGE = 50
  const DEFAULT_RELAY = 'https://mail.dmsg.net'
  let agent = $state('')
  let folder = $state('received')
  let mailboxes = $state<any[]>([])
  let push = $state<any>(null)
  let boxesLoaded = $state(false)
  let loadError = $state('')
  let messages = $state<any[] | null>(null)
  let selected = $state('')
  let cursor = $state<string | null>(null)
  let unread = $state(0)
  let routes = $state(DEFAULT_RELAY)
  let synced = $state('')
  let closing = $state<any>(null)
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)
  let refreshPending = false
  const keyOf = (message: any) => `${message.direction}:${message.from}:${message.message_id}`
  const current = $derived(messages?.find((message) => keyOf(message) === selected))

  $effect(() => {
    const preferred = active ?? agents[0]?.agent_id
    if (preferred) untrack(() => { agent = preferred })
  })

  onMount(() => window.dmsg.onEvent((event) => {
    const data = event.data as { agent?: string } | undefined
    if (event.type !== 'resync' && !(['agent/mail', 'agent/mail_push'].includes(event.type) && data?.agent === agent)) return
    if (!unlocked || !agent) return
    if (busy) refreshPending = true
    else void loadMail(false)
  }))
  $effect(() => {
    if (unlocked && agent && folder)
      untrack(() => {
        if (agent !== previousAgent) {
          closing = null; mailboxes = []; boxesLoaded = false; push = null; routes = DEFAULT_RELAY
          error = ''; notice = ''; loadError = ''; refreshPending = false
        }
        messages = null; cursor = null; unread = 0
        selected = ''
        synced = ''
        syncing = receiving.has(agent)
        void loadMail(agent !== previousAgent)
        previousAgent = agent
      })
  })

  async function tool(name: string, input: Record<string, unknown> = {}, target = agent) {
    const answer: any = await call('ap/call', { agent: target, tool: `agent_protocols_${name}`, input })
    if (answer.error) throw new CallError(String(answer.error.code).toUpperCase())
    return answer.result
  }

  function mailError(e: any) {
    return code(e) === 'LOCKED' ? t('agentLocked') : errorText(e) || t('agentLoadFailed')
  }

  async function attempt(work: () => Promise<void>) {
    if (!alive) return
    const turn = ++operation
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e: any) {
      if (!alive || turn !== operation) return
      error = code(e) === 'LOCKED' ? t('locked') : mailError(e)
    } finally {
      if (alive && turn === operation) {
        busy = false
        if (refreshPending) {
          refreshPending = false
          void loadMail(false)
        }
      }
    }
  }

  // The connector's cursor walks oldest to newest; filters apply on every page.
  async function list(more = false, target = agent, box = folder) {
    const page = await tool('mail_list', {
      limit: PAGE,
      direction: box === 'sent' ? 'sent' : 'received',
      ...(box === 'unread' ? { unread_only: true } : {}),
      ...(more && cursor ? { cursor } : {})
    }, target)
    if (!alive || agent !== target || folder !== box) return
    messages = more ? [...(messages ?? []), ...page.messages] : page.messages
    cursor = page.next_cursor ?? null
    unread = page.unread_count
    if (!messages?.some((message) => keyOf(message) === selected)) selected = ''
  }

  const refreshBoxes = async (target = agent) => {
    await Promise.all([
      tool('mail_mailboxes_list', {}, target).then((result) => { if (alive && agent === target) { mailboxes = result.mailboxes; boxesLoaded = true } }),
      call('agent/mail_push', { agent: target }).then((status) => { if (alive && agent === target) push = status })
    ])
  }

  const togglePush = () => attempt(async () => {
    const target = agent
    if (!push) return
    const status = await call('agent/mail_push_set', { agent: target, enabled: !push.enabled })
    if (alive && agent === target) push = status
  })
  const loadSaved = single(() => attempt(async () => {
    const target = agent
    const box = folder
    try {
      await Promise.all([refreshBoxes(target), list(false, target, box)])
      if (alive && agent === target && folder === box) loadError = ''
    } catch (e) {
      if (alive && agent === target && folder === box) loadError = mailError(e)
    }
  }))

  async function receive(target: string) {
    if (receiving.has(target)) return
    receiving.add(target)
    syncing = true
    try {
      const report = await tool('mail_sync', {}, target)
      if (!alive || agent !== target) return
      synced = `${t('mailReceived')} ${report.received.length} · ${t('mailDelivered')} ${report.delivered.length} · ${t('mailQueued')} ${report.queued}`
      await loadSaved()
      if (alive && agent === target && report.errors.length) error = report.errors.map((e: any) => `${e.route}: ${e.code}`).join('\n')
    } catch (e: any) {
      if (alive && agent === target) error = mailError(e)
    } finally {
      receiving.delete(target)
      if (alive && agent === target) syncing = false
    }
  }
  function loadMail(sync = true) {
    if (!alive) return
    const loaded = loadSaved()
    if (sync) void receive(agent)
    return loaded
  }

  async function publishBox(input: Record<string, unknown>, done: string) {
    const target = agent
    let result: any
    try {
      result = await tool('mail_mailbox_publish', input, target)
    } catch (failure) {
      // An unknown reply may already have a durable pending publication.
      if (alive && agent === target) await refreshBoxes(target).catch(() => {})
      throw failure
    }
    if (!alive || agent !== target) return
    await refreshBoxes(target).catch(e => { if (alive && agent === target) error = errorText(e) })
    if (result.failed?.length) error = result.failed.map((failure: any) => `${failure.route}: ${failure.code}`).join('\n')
    else notice = done
  }

  const publish = () =>
    attempt(async () => {
      const publishingAgent = agent, submittedRoutes = routes
      await publishBox(
        {
          routes: routes
            .split(',')
            .map((r) => r.trim())
            .filter(Boolean)
        },
        t('mailboxCreated')
      )
      if (alive && agent === publishingAgent && routes === submittedRoutes) routes = DEFAULT_RELAY
    })
  const recover = () => attempt(async () => {
    const target = agent
    const complete = await recoverMailboxes(routes.split(','), (input) => tool('mail_mailboxes_list', input, target), () => alive && agent === target)
    if (!complete) return
    await refreshBoxes(target)
    if (alive && agent === target) notice = t('mailboxesRecovered')
  })
  const replace = (box: any, change: Record<string, unknown>, done: string) => publishBox({
    mailbox_id: box.mailbox_id,
    routes: box.routes,
    ...(box.key_available === false && Array.isArray(change.routes) && change.routes.length === 0 ? { rotate_key: true } : {}),
    ...change
  }, done)
  const textOf = (message: any) =>
    message.parts
      .filter((part: any) => part.text)
      .map((part: any) => part.text)
      .join('\n')
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did
</script>

{#if !unlocked || !agents.length}
  <Notice>{unlocked ? t('noAgents') : t('locked')}</Notice>
{:else}
  <Section title={t('mail')} description={t('mailHint')} icon="mail" testid="agent-mail">
    {#snippet actions()}
      <Select
        value={agent}
        onChange={selectAgent}
        disabled={busy}
        label={t('tabAgentIds')}
        testid="mail-agent"
        options={agents.map((a) => ({ value: a.agent_id, label: a.label }))}
      />
    {/snippet}
    <div class="grid grid-cols-1 gap-4">
      <div class="flex items-start gap-3 rounded-xl bg-surface-2 px-4 py-3">
        <Icon name="lock" class="mt-0.5 shrink-0 text-forest" />
        <div class="text-callout leading-relaxed text-muted">
          <p class="font-medium text-ink">{t('mailExplained')}</p>
          <p class="mt-1">{t('mailOwnerRole')}</p>
        </div>
      </div>
      {#if loadError}<Notice tone="danger" testid="mail-load-error">{loadError}
        {#snippet action()}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => loadMail(false)}>{t('retry')}</button>{/snippet}
      </Notice>{/if}
      {#if error}<Notice failure={error} tone="danger" testid="mail-error">{error}</Notice>{/if}
      {#if notice}<Notice tone="ok">{notice}</Notice>{/if}
      <div class="flex flex-wrap items-center gap-3 rounded-xl border border-line px-4 py-3" data-testid="mail-push">
        <div class="min-w-0 flex-1"><p class="text-body font-medium">{t('mailPushTitle')}</p><p class="text-callout text-muted">{t('mailPushHint')}</p>
          {#if push?.enabled}<p class="mt-1 text-callout text-muted">{push.last_error ? t('mailPushFallback') : push.subscriptions?.some((s: any) => s.id && s.refresh_before > Date.now()) ? t('mailPushReady') : t('mailPushStarting')}</p>{/if}
        </div>
        <button class={buttonClass('secondary','sm')} disabled={busy || !push} aria-pressed={push?.enabled ?? false} onclick={togglePush} data-testid="mail-push-toggle">{push?.enabled ? t('mailPushDisable') : t('mailPushEnable')}</button>
        <button class={buttonClass('ghost','sm')} onclick={manage}>{t('tabCapabilities')}</button>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Segmented
          bind:value={folder}
          disabled={busy}
          label={t('letters')}
          testid="mail-folder"
          options={[
            { value: 'received', label: t('mailReceivedTab') },
            { value: 'sent', label: t('mailSentTab') },
            { value: 'unread', label: `${t('mailAgentUnread')} · ${unread}` }
          ]}
        />
        <span class="flex-1"></span>
        <button class={buttonClass('secondary', 'sm')} disabled={busy || syncing} data-testid="mail-sync" onclick={() => loadMail()}><Icon name="cloud-down" />{t('reload')}</button>
      </div>
      {#if synced}<p class="text-callout text-muted" role="status">{synced}</p>{/if}
      {#if messages?.length}
        <div class="grid min-h-80 lg:grid-cols-[minmax(220px,0.85fr)_minmax(0,1.5fr)]">
          <div class="min-w-0 border-b border-line lg:border-e lg:border-b-0">
            <ul class="max-h-[520px] divide-y divide-line overflow-y-auto" data-testid="mail-messages">
              {#each messages as message (keyOf(message))}
                <li>
                  <button
                    class={cn(
                      'grid w-full gap-1.5 px-4 py-3 text-start outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-forest',
                      selected === keyOf(message) && 'bg-pale'
                    )}
                    aria-pressed={selected === keyOf(message)}
                    onclick={() => (selected = keyOf(message))}
                  >
                    <span class="flex items-center gap-2">
                      <span class="min-w-0 flex-1 truncate text-body font-medium">{message.subject || t('mailNoSubject')}</span>
                      {#if message.read === false}<span class="size-1.5 shrink-0 rounded-full bg-forest" title={t('mailAgentUnread')}></span>{/if}
                    </span>
                    <span class="truncate text-caption text-muted">{labelOf(message.direction === 'received' ? message.from : message.to)}</span>
                    <span class="truncate text-callout text-muted">{textOf(message) || t('attachments')}</span>
                    <span class="text-caption text-faint">{ago(message.received_at ?? message.created_at)}</span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if cursor}<button class={buttonClass('ghost', 'sm', 'm-2')} disabled={busy} onclick={() => attempt(() => list(true))} data-testid="mail-more"
                >{t('agentLoadMore')}</button
              >{/if}
          </div>
          <div class="min-w-0 p-5" data-testid="mail-detail">
            {#if current}
              <div class="mb-3 flex flex-wrap items-center gap-2">
                <Pill tone="forest">{current.direction === 'received' ? t('mailReceivedTab') : t('mailSentTab')}</Pill>
                {#if current.direction === 'received'}<Pill>{current.read === false ? t('mailAgentUnread') : t('mailAgentRead')}</Pill>{/if}
                {#if current.delivery}<Pill tone={current.delivery.status === 'queued' ? 'warn' : 'neutral'}
                    >{current.delivery.status === 'queued' ? t('mailQueued') : t('mailDelivered')}</Pill
                  >{/if}
              </div>
              {#if current.delivery?.status === 'delivered'}<p class="mb-3 text-caption text-muted">{t('mailDeliveryHint')}</p>{/if}
              <h3 class="text-title-2 leading-snug font-semibold [overflow-wrap:anywhere]">{current.subject || t('mailNoSubject')}</h3>
              <dl class="mt-4 grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-3 gap-y-2 text-callout">
                <dt class="text-faint">{t('from')}</dt>
                <dd><Id id={current.from} /></dd>
                <dt class="text-faint">{t('to')}</dt>
                <dd><Id id={current.to} /></dd>
                <dt class="text-faint">{t('createdAt')}</dt>
                <dd>{when(current.created_at)}</dd>
              </dl>
              <div class="mt-5 border-t border-line pt-4 text-body leading-relaxed">
                {#each current.parts as part}
                  {#if part.text}<p class="mb-3 whitespace-pre-wrap select-text [overflow-wrap:anywhere]">{part.text}</p>
                  {:else}<div class="mb-2 flex items-center gap-2 rounded-lg bg-surface-2 p-2.5 text-callout">
                      <Icon name="attach" /><span class="min-w-0 break-all">{part.name ?? part.media_type} · {part.size} B</span>
                    </div>{/if}
                {/each}
              </div>
              {#if current.delivery?.last_error}<Notice tone="warn">{current.delivery.last_error.code}: {current.delivery.last_error.message}</Notice>{/if}
              <details class="mt-4 text-caption text-muted">
                <summary>{t('agentTechnicalDetails')}</summary>
                <div class="mt-2 grid gap-2"><Id id={current.message_id} /><Id id={current.thread_id} /><span>{t('expires')} {when(current.expires_at)}</span></div>
              </details>
            {:else}
              <div class="grid min-h-64 place-content-center gap-3 text-center">
                <Icon name="mail" class="mx-auto size-7 text-faint" />
                <p class="text-body text-muted">{t('mailSelect')}</p>
              </div>
            {/if}
          </div>
        </div>
      {:else if messages || (!error && !loadError)}
        <div class="grid min-h-60 place-content-center justify-items-center gap-3 px-8 text-center" data-testid="mail-empty">
          <Icon name="mail" class="size-8 text-faint" />
          <h3 class="text-body font-medium">{busy ? t('loading') : error ? t('agentLoadFailed') : t('noLetters')}</h3>
          <p class="max-w-md text-callout leading-relaxed text-muted">{t('mailEmptyHint')}</p>
        </div>
      {/if}
      <p class="text-caption text-faint">{t('mailReadHint')}</p>
      <details class="rounded-xl border border-line" data-testid="mailbox-settings">
        <summary class="px-4 py-3 text-callout font-medium">{t('mailboxManagement')}{boxesLoaded ? ' · ' + mailboxes.length : ''}</summary>
        <div class="grid grid-cols-1 gap-3 border-t border-line p-4">
          <ul class="grid grid-cols-1 gap-2" data-testid="mailboxes">
            {#each mailboxes as box (box.mailbox_id)}
              <li class="grid grid-cols-1 gap-2 rounded-lg bg-surface-2 p-3 text-callout">
                <div class="flex items-center gap-2">
                  <bdi dir="ltr" class="min-w-0 flex-1 truncate font-mono">{box.address}</bdi><button
                    class={buttonClass('ghost', 'xs')}
                    aria-label={t('copy')}
                    onclick={() => copyText(box.address)}><Icon name="copy" /></button
                  ><Pill tone={box.status === 'open' ? 'ok' : 'neutral'}
                    >{box.status === 'open' ? t('mailboxOpen') : box.status === 'expired' ? stateLabel('expired') : t('mailboxInactive')}</Pill
                  >
                </div>
                <span class="text-faint">{t('expires')} {when(box.expires_at)}</span>
                {#if box.key_available === false}<Notice tone="warn" testid="mailbox-key-missing">{t('mailboxKeyMissing')}</Notice>{/if}
                {#if box.pending_routes?.length}<p class="text-muted">{t('mailboxPublicationPending')}: {box.pending_routes.join(', ')}</p>{/if}
                <div class="flex flex-wrap gap-2">
                  {#if box.status !== 'closed'}
                    <button class={buttonClass('subtle', 'xs')} disabled={busy || box.key_available === false} onclick={() => attempt(() => replace(box, {}, t('mailboxRenewed')))} data-testid="mailbox-renew"
                      >{t('renew')}</button
                    >
                    <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => attempt(() => replace(box, { rotate_key: true }, t('mailboxRotated')))} data-testid="mailbox-rotate"
                      >{t('rotateKey')}</button
                    >
                    <button class={buttonClass('ghost', 'xs', 'text-danger')} disabled={busy} onclick={() => (closing = box)} data-testid="mailbox-close"
                      >{t('closeMailbox')}</button
                    >
                  {/if}
                </div>
              </li>
            {:else}<li class="text-callout text-muted">{boxesLoaded ? t('noMailboxes') : loadError ? '' : t('loading')}</li>{/each}
          </ul>
          <div class="flex flex-wrap gap-2">
            <input class="min-w-48 flex-1" aria-label={t('mailRoutes')} placeholder={t('mailRoutes')} bind:value={routes} data-testid="mail-routes" /><button
              class={buttonClass('secondary')}
              disabled={busy || !routes.trim()}
              onclick={publish}>{t('openMailbox')}</button
            >
            <button class={buttonClass('secondary')} disabled={busy || !routes.trim()} onclick={recover} data-testid="mailbox-recover">{t('recoverMailboxes')}</button>
          </div>
          <details class="text-callout text-muted" data-testid="mail-blocked">
            <summary>{t('blockedSenders', { n: blocked.length })}</summary>
            <p class="mt-2">{t('blockedSendersHint')}</p>
            <ul class="mt-2 grid gap-1 font-mono text-code">
              {#each blocked as sender (sender)}<li class="break-all">{sender}</li>{/each}
            </ul>
          </details>
        </div>
      </details>
      <div class="flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
        <span class="text-callout text-muted">{t('agentPermissionsHint')}</span><button class={buttonClass('ghost', 'sm')} onclick={manage}
          ><Icon name="security" />{t('tabCapabilities')}</button
        >
      </div>
    </div>
  </Section>
  <Confirm
    bind:open={() => !!closing, (open) => !open && (closing = null)}
    title={t('closeMailboxTitle')}
    description={t('closeMailboxHint')}
    confirm={t('closeMailbox')}
    {busy}
    testid="mailbox-close-dialog"
    onConfirm={() => replace(closing, { routes: [] }, t('mailboxClosed'))}
    onReview={loadSaved}
    describeError={mailError}
  />
{/if}
