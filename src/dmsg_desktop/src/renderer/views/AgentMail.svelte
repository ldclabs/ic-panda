<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Select from '../components/Select.svelte'

  /** Agent Mail and Agent Knowledge, run as a hosted Agent ID with the connector's own tools. */
  let { agents, active }: { agents: any[]; active: string | null } = $props()
  let agent = $state('')
  let mailboxes = $state<any[]>([])
  let messages = $state<any[]>([])
  let unread = $state(0)
  let routes = $state('')
  let synced = $state('')
  let draft = $state({ to: '', subject: '', text: '', in_reply_to: '', reply_mailbox_id: '' })
  let service = $state('')
  let actor = $state('')
  let items = $state<any[]>([])
  let lookup = $state('')
  let found = $state<any>(null)
  let kind = $state('knowledge.publish')
  let payload = $state('')
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)

  $effect(() => {
    if (!agent && agents.length) agent = active ?? agents[0].agent_id
  })

  /** A standard tool's answer is `{result}` or `{error: {code, message}}`. */
  async function tool(name: string, input: Record<string, unknown> = {}) {
    const answer: any = await call('ap/call', { agent, tool: `agent_protocols_${name}`, input })
    if (answer.error) throw Object.assign(new Error(answer.error.message), { tool: answer.error.code })
    return answer.result
  }

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e: any) {
      error = `${t('error')}: ${e?.tool ?? (code(e) === 'LOCKED' ? t('locked') : code(e))}${e?.tool ? ` ${e.message}` : ''}`
    } finally {
      busy = false
    }
  }

  const loadMail = () =>
    attempt(async () => {
      mailboxes = (await tool('mail_mailboxes_list')).mailboxes
      const page = await tool('mail_list', { limit: 50 })
      messages = page.messages
      unread = page.unread_count
    })

  const publish = () =>
    attempt(async () => {
      const list = routes
        .split(',')
        .map((r) => r.trim())
        .filter(Boolean)
      await tool('mail_mailbox_publish', { routes: list })
      routes = ''
      mailboxes = (await tool('mail_mailboxes_list')).mailboxes
    })

  const sync = () =>
    attempt(async () => {
      const report = await tool('mail_sync')
      synced = `${t('mailReceived')} ${report.received.length} · ${t('mailDelivered')} ${report.delivered.length} · ${t('mailQueued')} ${report.queued.length}${report.errors.length ? ` · ${t('error')} ${report.errors.map((e: any) => e.code).join(', ')}` : ''}`
      const page = await tool('mail_list', { limit: 50 })
      messages = page.messages
      unread = page.unread_count
    })

  const markRead = () =>
    attempt(async () => {
      const page = await tool('mail_list', { unread_only: true, mark_read: true, limit: 50 })
      unread = page.unread_count
      messages = (await tool('mail_list', { limit: 50 })).messages
    })

  const send = () =>
    attempt(async () => {
      const input: Record<string, unknown> = { text: draft.text }
      if (draft.to.trim()) input.to = draft.to.trim()
      if (draft.subject.trim()) input.subject = draft.subject.trim()
      if (draft.in_reply_to) input.in_reply_to = draft.in_reply_to
      if (draft.reply_mailbox_id) input.reply_mailbox_id = draft.reply_mailbox_id
      const sent = await tool('mail_send', input)
      notice = `${t('mailSent')}: ${sent.status}`
      draft = { to: '', subject: '', text: '', in_reply_to: '', reply_mailbox_id: draft.reply_mailbox_id }
      messages = (await tool('mail_list', { limit: 50 })).messages
    })

  const reply = (message: any) => {
    draft = { ...draft, to: '', in_reply_to: message.message_id, subject: message.subject ? `Re: ${message.subject}` : '' }
  }

  const query = () =>
    attempt(async () => {
      const input: Record<string, unknown> = { service: service.trim(), limit: 20 }
      if (actor.trim()) input.actor = actor.trim()
      items = (await tool('knowledge_query', input)).items
    })

  const get = (id: string) =>
    attempt(async () => {
      const got = (await tool('knowledge_get', { service: service.trim(), ids: [id] })).items[0]
      const status = await tool('knowledge_status', { service: service.trim(), id })
      found = { ...got, status: status.status }
    })

  const submit = () =>
    attempt(async () => {
      let parsed: unknown
      try {
        parsed = JSON.parse(payload)
      } catch {
        throw Object.assign(new Error('payload'), { tool: 'invalid_payload' })
      }
      const submitted = await tool('knowledge_submit', { services: [service.trim()], type: kind, payload: parsed })
      notice = `${t('knowledgeSubmitted')}: ${submitted.id}`
      payload = ''
    })

  const textOf = (message: any) =>
    message.parts
      .filter((p: any) => p.text)
      .map((p: any) => p.text)
      .join('\n')
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did
</script>

<div class="card" data-testid="agent-mail">
  <div class="row spread">
    <h2>{t('mail')}</h2>
    <Select
      bind:value={agent}
      onChange={() => ((mailboxes = []), (messages = []))}
      testid="mail-agent"
      options={agents.map((a) => ({ value: a.agent_id, label: a.label }))} />
  </div>
  <p class="muted">{t('mailHint')}</p>
  {#if error}<p class="error" data-testid="mail-error">{error}</p>{/if}
  {#if notice}<p class="muted">{notice}</p>{/if}
  <div class="row">
    <button disabled={busy || !agent} data-testid="mail-open" onclick={loadMail}>{t('open')}</button>
    <button disabled={busy || !agent} onclick={sync}>{t('sync')}</button>
    {#if unread}<button disabled={busy} onclick={markRead}>{t('markRead')} ({unread})</button>{/if}
    {#if synced}<span class="muted">{synced}</span>{/if}
  </div>
  <h3>{t('mailboxes')}</h3>
  <ul class="list" data-testid="mailboxes">
    {#each mailboxes as box (box.mailbox_id)}
      <li>
        <div class="mono">{box.address}</div>
        <div class="muted">{box.status} · {t('unread')} {box.unread_count} · {t('expires')} {when(box.expires_at)}</div>
      </li>
    {:else}
      <li class="muted">{t('noMailboxes')}</li>
    {/each}
  </ul>
  <div class="row" style="margin-top: 8px">
    <input placeholder={t('mailRoutes')} bind:value={routes} data-testid="mail-routes" />
    <button disabled={busy || !routes.trim()} onclick={publish}>{t('openMailbox')}</button>
  </div>
  <h3>{t('letters')}</h3>
  <ul class="list" data-testid="mail-messages">
    {#each messages as message (message.message_id)}
      <li>
        <div class="row spread">
          <span>
            <span class="badge">{message.direction === 'received' ? t('mailIn') : t('mailOut')}</span>
            {#if message.read === false}<strong>·</strong>{/if}
            {message.subject ?? ''}
            <span class="mono muted">{labelOf(message.direction === 'received' ? message.from : message.to)}</span>
          </span>
          <span class="row">
            <span class="muted">{when(message.received_at ?? message.created_at)}</span>
            {#if message.direction === 'received' && message.reply_address}<button disabled={busy} onclick={() => reply(message)}>{t('reply')}</button>{/if}
          </span>
        </div>
        <div class="body">{textOf(message)}</div>
      </li>
    {:else}
      <li class="muted">{t('noLetters')}</li>
    {/each}
  </ul>
  <div class="form">
    {#if draft.in_reply_to}
      <p class="muted">{t('replyingTo')} <span class="mono">{draft.in_reply_to.slice(0, 16)}</span> <button onclick={() => (draft.in_reply_to = '')}>{t('cancel')}</button></p>
    {:else}
      <input placeholder={t('mailTo')} bind:value={draft.to} data-testid="mail-to" />
    {/if}
    <input placeholder={t('subject')} bind:value={draft.subject} />
    <textarea placeholder={t('message')} bind:value={draft.text} data-testid="mail-text"></textarea>
    <div class="row">
      <Select
        bind:value={draft.reply_mailbox_id}
        options={[
          { value: '', label: t('noReplyCard') },
          ...mailboxes
            .filter((b) => b.status === 'open')
            .map((box) => ({ value: box.mailbox_id, label: `${t('replyCard')} ${box.mailbox_id.slice(0, 8)}` }))
        ]} />
      <button class="primary" disabled={busy || !draft.text.trim() || (!draft.to.trim() && !draft.in_reply_to)} data-testid="mail-send" onclick={send}>{t('send')}</button>
    </div>
  </div>
</div>
<div class="card" data-testid="agent-knowledge">
  <h2>{t('knowledge')}</h2>
  <p class="muted">{t('knowledgeHint')}</p>
  <div class="row">
    <input placeholder={t('knowledgeService')} bind:value={service} data-testid="knowledge-service" />
    <input placeholder={t('knowledgeActor')} bind:value={actor} />
    <button disabled={busy || !agent || !service.trim()} onclick={query}>{t('lookUp')}</button>
  </div>
  <ul class="list" data-testid="knowledge-items">
    {#each items as item (item.id)}
      <li class="row spread">
        <span>
          <strong>{item.payload?.title ?? item.type}</strong>
          <span class="mono muted">{labelOf(item.actor)} · {when(item.created_at)}</span>
        </span>
        <button disabled={busy} onclick={() => get(item.id)}>{t('open')}</button>
      </li>
    {/each}
  </ul>
  <div class="row" style="margin-top: 8px">
    <input placeholder="ID" bind:value={lookup} />
    <button disabled={busy || !service.trim() || !lookup.trim()} onclick={() => get(lookup.trim())}>{t('open')}</button>
  </div>
  {#if found}
    <dl style="margin-top: 8px" data-testid="knowledge-found">
      <dt>ID</dt>
      <dd class="mono">{found.id}</dd>
      <dt>{t('status')}</dt>
      <dd>{found.status}</dd>
      <dt>{t('author')}</dt>
      <dd class="mono">{labelOf(found.actor)}</dd>
      <dt>{t('content')}</dt>
      <dd><pre class="mono">{JSON.stringify(found.payload, null, 2)}</pre></dd>
    </dl>
  {/if}
  <h3>{t('knowledgeSubmit')}</h3>
  <div class="form">
    <Select
      bind:value={kind}
      class="kind"
      options={['knowledge.publish', 'knowledge.retract'].map((k) => ({ value: k, label: k, mono: true }))} />
    <textarea placeholder={t('knowledgePayload')} bind:value={payload}></textarea>
    <button class="primary" disabled={busy || !agent || !service.trim() || !payload.trim()} onclick={submit}>{t('submitTo')}</button>
  </div>
</div>

<style>
  h3 {
    margin-top: 12px;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }
  .body {
    white-space: pre-wrap;
  }
  .form :global(.kind) {
    align-self: flex-start;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
  }
</style>
