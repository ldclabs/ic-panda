<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { ago, call, code, when } from '../../rpc'

  /** Agent Mail and Agent Knowledge, run as a hosted Agent ID with the connector's own tools. */
  let { agents, active, unlocked }: { agents: any[]; active: string | null; unlocked: boolean } = $props()

  let agent = $state('')
  let mailboxes = $state<any[]>([])
  let messages = $state<any[] | null>(null)
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

  async function list() {
    const page = await tool('mail_list', { limit: 50 })
    messages = page.messages
    unread = page.unread_count
  }

  const loadMail = () =>
    attempt(async () => {
      mailboxes = (await tool('mail_mailboxes_list')).mailboxes
      await list()
    })

  const publish = () =>
    attempt(async () => {
      await tool('mail_mailbox_publish', { routes: routes.split(',').map((r) => r.trim()).filter(Boolean) })
      routes = ''
      mailboxes = (await tool('mail_mailboxes_list')).mailboxes
    })

  const sync = () =>
    attempt(async () => {
      const report = await tool('mail_sync')
      synced = `${t('mailReceived')} ${report.received.length} · ${t('mailDelivered')} ${report.delivered.length} · ${t('mailQueued')} ${report.queued.length}${report.errors.length ? ` · ${t('error')} ${report.errors.map((e: any) => e.code).join(', ')}` : ''}`
      await list()
    })

  const markRead = () =>
    attempt(async () => {
      await tool('mail_list', { unread_only: true, mark_read: true, limit: 50 })
      await list()
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
      await list()
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

{#if !unlocked || !agents.length}
  <Notice>{unlocked ? t('noAgents') : t('locked')}</Notice>
{:else}
  <Section title={t('mail')} description={t('mailHint')} icon="mail" testid="agent-mail">
    {#snippet actions()}
      <Select bind:value={agent} onChange={() => ((mailboxes = []), (messages = null))} testid="mail-agent" options={agents.map((a) => ({ value: a.agent_id, label: a.label }))} />
      <button class={buttonClass('primary', 'md')} disabled={busy || !agent} data-testid="mail-open" onclick={loadMail}>{t('open')}</button>
    {/snippet}
    <div class="grid gap-3">
      {#if error}<Notice tone="danger" testid="mail-error">{error}</Notice>{/if}
      {#if notice}<Notice tone="ok">{notice}</Notice>{/if}
      {#if messages}
        <div class="flex flex-wrap items-center gap-1.5">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={sync}><Icon name="sync-now" />{t('sync')}</button>
          {#if unread}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={markRead}><Icon name="check" />{t('markRead')} · {unread}</button>{/if}
          {#if synced}<span class="text-[12px] text-faint">{synced}</span>{/if}
        </div>
        <div>
          <h3 class="mb-1.5 text-[12px] font-medium text-faint">{t('mailboxes')}</h3>
          <ul class="grid gap-1.5" data-testid="mailboxes">
            {#each mailboxes as box (box.mailbox_id)}
              <li class="flex flex-wrap items-center gap-2 rounded-lg border border-line px-3 py-2 text-[12.5px]">
                <bdi dir="ltr" class="min-w-0 flex-1 truncate font-mono">{box.address}</bdi>
                <Pill tone={box.status === 'open' ? 'ok' : 'neutral'}>{box.status}</Pill>
                <span class="text-faint">{t('unread')} {box.unread_count} · {t('expires')} {when(box.expires_at)}</span>
              </li>
            {:else}
              <li class="text-[12.5px] text-faint">{t('noMailboxes')}</li>
            {/each}
          </ul>
          <div class="mt-2 flex gap-2">
            <input placeholder={t('mailRoutes')} bind:value={routes} data-testid="mail-routes" />
            <button class={buttonClass('secondary')} disabled={busy || !routes.trim()} onclick={publish}>{t('openMailbox')}</button>
          </div>
        </div>
        <ul class="divide-y divide-line rounded-xl border border-line" data-testid="mail-messages">
          {#each messages as message (message.message_id)}
            <li class="grid gap-1 px-3.5 py-3">
              <div class="flex flex-wrap items-center gap-2">
                <Pill tone={message.direction === 'received' ? 'forest' : 'neutral'}>{message.direction === 'received' ? t('mailIn') : t('mailOut')}</Pill>
                {#if message.read === false}<span class="size-1.5 rounded-full bg-forest" title={t('unread')}></span>{/if}
                <b class={cn('min-w-0 flex-1 truncate text-[13px]', message.read === false ? 'font-semibold' : 'font-medium')}>{message.subject ?? ''}</b>
                <span class="text-[11.5px] text-faint" title={when(message.received_at ?? message.created_at)}>{ago(message.received_at ?? message.created_at)}</span>
                {#if message.direction === 'received' && message.reply_address}
                  <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => reply(message)}><Icon name="reply" />{t('reply')}</button>
                {/if}
              </div>
              <span class="font-mono text-[11.5px] text-faint">{labelOf(message.direction === 'received' ? message.from : message.to)}</span>
              <p class="text-[13px] whitespace-pre-wrap [overflow-wrap:anywhere]">{textOf(message)}</p>
            </li>
          {:else}
            <li class="px-3.5 py-3 text-[12.5px] text-faint">{t('noLetters')}</li>
          {/each}
        </ul>
        <div class="grid gap-2 rounded-xl border border-line bg-surface-2/60 p-3">
          {#if draft.in_reply_to}
            <div class="flex items-center gap-2 text-[12px] text-muted">
              <Icon name="reply" />{t('replyingTo')} <span class="font-mono">{draft.in_reply_to.slice(0, 16)}</span>
              <button class={buttonClass('ghost', 'xs')} onclick={() => (draft.in_reply_to = '')}>{t('cancel')}</button>
            </div>
          {:else}
            <Field label={t('mailTo')}><input bind:value={draft.to} class="font-mono text-[12.5px]" data-testid="mail-to" /></Field>
          {/if}
          <Field label={t('subject')}><input bind:value={draft.subject} /></Field>
          <Field label={t('message')}><textarea bind:value={draft.text} rows="4" data-testid="mail-text"></textarea></Field>
          <div class="flex flex-wrap items-center justify-end gap-2">
            <Select
              bind:value={draft.reply_mailbox_id}
              options={[
                { value: '', label: t('noReplyCard') },
                ...mailboxes.filter((b) => b.status === 'open').map((box) => ({ value: box.mailbox_id, label: `${t('replyCard')} ${box.mailbox_id.slice(0, 8)}` }))
              ]}
            />
            <button class={buttonClass('primary')} disabled={busy || !draft.text.trim() || (!draft.to.trim() && !draft.in_reply_to)} data-testid="mail-send" onclick={send}>
              <Icon name="send" class="rtl:-scale-x-100" />{t('send')}
            </button>
          </div>
        </div>
      {/if}
    </div>
  </Section>

  <Section title={t('knowledge')} description={t('knowledgeHint')} icon="knowledge" testid="agent-knowledge">
    <div class="grid gap-3">
      <div class="grid gap-2 sm:grid-cols-[1fr_1fr_auto] sm:items-end">
        <Field label={t('knowledgeService')}><input bind:value={service} class="font-mono text-[12.5px]" data-testid="knowledge-service" /></Field>
        <Field label={t('knowledgeActor')}><input bind:value={actor} class="font-mono text-[12.5px]" /></Field>
        <button class={buttonClass('secondary')} disabled={busy || !agent || !service.trim()} onclick={query}><Icon name="search" />{t('lookUp')}</button>
      </div>
      {#if items.length}
        <ul class="divide-y divide-line rounded-xl border border-line" data-testid="knowledge-items">
          {#each items as item (item.id)}
            <li class="flex items-center gap-2 px-3.5 py-2.5">
              <span class="min-w-0 flex-1">
                <b class="block truncate text-[13px] font-medium">{item.payload?.title ?? item.type}</b>
                <span class="font-mono text-[11.5px] text-faint">{labelOf(item.actor)} · {when(item.created_at)}</span>
              </span>
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => get(item.id)}>{t('open')}</button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="flex gap-2">
        <input placeholder="ID" bind:value={lookup} class="font-mono text-[12.5px] placeholder:font-sans" />
        <button class={buttonClass('secondary')} disabled={busy || !service.trim() || !lookup.trim()} onclick={() => get(lookup.trim())}>{t('open')}</button>
      </div>
      {#if found}
        <div class="grid gap-2 rounded-xl border border-line p-3" data-testid="knowledge-found">
          <div class="flex flex-wrap items-center gap-2">
            <Id id={found.id} />
            <Pill>{found.status}</Pill>
            <span class="font-mono text-[11.5px] text-faint">{labelOf(found.actor)}</span>
          </div>
          <pre class="overflow-x-auto rounded-lg bg-surface-2 p-2.5 font-mono text-[11.5px] whitespace-pre-wrap">{JSON.stringify(found.payload, null, 2)}</pre>
        </div>
      {/if}
      <div class="grid gap-2 rounded-xl border border-line bg-surface-2/60 p-3">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-[12.5px] font-medium">{t('knowledgeSubmit')}</h3>
          <Select bind:value={kind} options={['knowledge.publish', 'knowledge.retract'].map((k) => ({ value: k, label: k, mono: true }))} />
        </div>
        <textarea placeholder={t('knowledgePayload')} bind:value={payload} rows="4" class="font-mono text-[12px]"></textarea>
        <button class={buttonClass('primary', 'md', 'justify-self-end')} disabled={busy || !agent || !service.trim() || !payload.trim()} onclick={submit}>{t('submitTo')}</button>
      </div>
    </div>
  </Section>
{/if}
