<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, code, when } from '../../rpc'

  /**
   * Agent Knowledge as a hosted Agent ID, with the connector's own tools:
   * exact queries page by page, a service's ranked search (relevance, not
   * correctness), one event with its retractions and assessments, and
   * submitting a contribution, an assessment or a retraction, which is
   * public and permanent.
   */
  let { agents, active, hosts, unlocked }: { agents: any[]; active: string | null; hosts: any[]; unlocked: boolean } = $props()

  const PAGE = 20
  /** Starting payloads for each event type (Agent Knowledge §5). */
  const templates: Record<string, unknown> = {
    'knowledge.publish': {
      license: 'CC-BY-4.0',
      kind: 'observation',
      title: '',
      statement: '',
      language: 'en',
      context: { scope: '', conditions: [], limitations: [] },
      basis: ''
    },
    'knowledge.assess': {
      license: 'CC-BY-4.0',
      target: '',
      verdict: 'supports',
      summary: '',
      context: { scope: '', conditions: [], limitations: [] },
      basis: ''
    },
    'knowledge.retract': { target: '', reason: '' }
  }

  let agent = $state('')
  let service = $state('')
  let mode = $state<'query' | 'search'>('search')
  let actor = $state('')
  let text = $state('')
  let searchMode = $state('hybrid')
  let items = $state<any[]>([])
  let cursor = $state<string | null>(null)
  let lookup = $state('')
  let found = $state<any>(null)
  let kind = $state('knowledge.publish')
  let payload = $state(JSON.stringify(templates['knowledge.publish'], null, 2))
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)

  $effect(() => {
    if (!agent && agents.length) agent = active ?? agents[0].agent_id
  })
  $effect(() => {
    if (!service && hosts.length) service = (hosts.find((h) => /knowledge/i.test(`${h.label ?? ''} ${h.host}`)) ?? hosts[0]).host
  })

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

  const look = (more = false) =>
    attempt(async () => {
      if (mode === 'search') {
        const result = await tool('knowledge_search', { service, text: text.trim(), mode: searchMode, limit: PAGE })
        items = result.hits.map((hit: any) => ({ ...hit.item, explanation: hit.explanation }))
        cursor = null
        return
      }
      const input: Record<string, unknown> = { service, limit: PAGE }
      if (actor.trim()) input.actor = actor.trim()
      if (more && cursor) input.cursor = cursor
      const result = await tool('knowledge_query', input)
      items = more ? [...items, ...result.items] : result.items
      cursor = result.next_cursor ?? null
    })

  const get = (id: string) =>
    attempt(async () => {
      const got = (await tool('knowledge_get', { service, ids: [id] })).items[0]
      if (!got) throw Object.assign(new Error(id), { tool: 'not_found' })
      const status = await tool('knowledge_status', { service, id })
      found = { ...got, status: status.status, assessments: status.assessments ?? [] }
    })

  const submit = () =>
    attempt(async () => {
      let parsed: unknown
      try {
        parsed = JSON.parse(payload)
      } catch {
        throw Object.assign(new Error('payload'), { tool: 'invalid_payload' })
      }
      const submitted = await tool('knowledge_submit', { services: [service], type: kind, payload: parsed })
      const refused = submitted.results?.filter((r: any) => r.error) ?? []
      notice = `${t('knowledgeSubmitted')}: ${submitted.id}${refused.length ? ` · ${t('error')} ${refused.map((r: any) => r.error.code).join(', ')}` : ''}`
    })

  /** Start an assessment or retraction of the event shown. */
  function answer(type: string) {
    kind = type
    payload = JSON.stringify({ ...(templates[type] as object), target: found.id }, null, 2)
  }

  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did
</script>

{#if !unlocked || !agents.length}
  <Notice>{unlocked ? t('noAgents') : t('locked')}</Notice>
{:else}
  <Section title={t('knowledge')} description={t('knowledgeHint')} icon="knowledge" testid="agent-knowledge">
    {#snippet actions()}
      <Select bind:value={agent} testid="knowledge-agent" options={agents.map((a) => ({ value: a.agent_id, label: a.label }))} />
    {/snippet}
    <div class="grid gap-3">
      {#if error}<Notice tone="danger" testid="knowledge-error">{error}</Notice>{/if}
      {#if notice}<Notice tone="ok">{notice}</Notice>{/if}
      <Field label={t('knowledgeService')}>
        <Select
          bind:value={service}
          testid="knowledge-service"
          class="w-full"
          options={hosts.map((h) => ({ value: h.host, label: h.label ? `${h.host} · ${h.label}` : h.host, mono: true }))}
        />
      </Field>
      <div class="flex flex-wrap items-end gap-2">
        <Segmented
          bind:value={() => mode, (next) => (mode = next as 'query' | 'search')}
          label={t('knowledge')}
          testid="knowledge-mode"
          options={[
            { value: 'search', label: t('knowledgeSearch') },
            { value: 'query', label: t('knowledgeQuery') }
          ]}
        />
        {#if mode === 'search'}
          <input class="min-w-0 flex-1" placeholder={t('knowledgeSearchText')} bind:value={text} data-testid="knowledge-text" />
          <Select bind:value={searchMode} options={['hybrid', 'lexical', 'semantic'].map((m) => ({ value: m, label: m }))} />
        {:else}
          <input class="min-w-0 flex-1 font-mono text-[12.5px] placeholder:font-sans" placeholder={t('knowledgeActor')} bind:value={actor} data-testid="knowledge-actor" />
        {/if}
        <button class={buttonClass('secondary')} disabled={busy || !agent || !service || (mode === 'search' && !text.trim())} onclick={() => look()} data-testid="knowledge-look"
          ><Icon name="search" />{t('lookUp')}</button
        >
      </div>
      {#if mode === 'search' && items.length}<p class="text-[11.5px] text-faint">{t('rankingNote')}</p>{/if}
      {#if items.length}
        <ul class="divide-y divide-line rounded-xl border border-line" data-testid="knowledge-items">
          {#each items as item (item.id)}
            <li class="flex items-center gap-2 px-3.5 py-2.5">
              <span class="min-w-0 flex-1">
                <b class="block truncate text-[13px] font-medium">{item.payload?.title ?? item.payload?.summary ?? item.type}</b>
                <span class="font-mono text-[11.5px] text-faint">{item.type} · {labelOf(item.actor)} · {when(item.created_at)}</span>
                {#if item.explanation}<span class="block text-[11.5px] text-muted">{item.explanation}</span>{/if}
              </span>
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => get(item.id)}>{t('open')}</button>
            </li>
          {/each}
        </ul>
        {#if cursor}<button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={busy} onclick={() => look(true)}>{t('older')}</button>{/if}
      {/if}
      <div class="flex gap-2">
        <input placeholder="ID" bind:value={lookup} class="font-mono text-[12.5px] placeholder:font-sans" />
        <button class={buttonClass('secondary')} disabled={busy || !service || !lookup.trim()} onclick={() => get(lookup.trim())}>{t('open')}</button>
      </div>
      {#if found}
        <div class="grid gap-2 rounded-xl border border-line p-3" data-testid="knowledge-found">
          <div class="flex flex-wrap items-center gap-2">
            <Id id={found.id} />
            <Pill>{found.status}</Pill>
            <span class="font-mono text-[11.5px] text-faint">{labelOf(found.actor)}</span>
            <span class="flex-1"></span>
            <button class={buttonClass('ghost', 'xs')} onclick={() => answer('knowledge.assess')}>{t('knowledgeAssess')}</button>
            {#if agents.some((a) => a.agent_id === found.actor)}<button class={buttonClass('ghost', 'xs')} onclick={() => answer('knowledge.retract')}>{t('knowledgeRetract')}</button>{/if}
          </div>
          <pre class="overflow-x-auto rounded-lg bg-surface-2 p-2.5 font-mono text-[11.5px] whitespace-pre-wrap">{JSON.stringify(found.payload, null, 2)}</pre>
          {#if found.assessments.length}
            <p class="text-[11.5px] text-faint">{t('assessments', { n: found.assessments.length })}</p>
          {/if}
        </div>
      {/if}
      <div class="grid gap-2 rounded-xl border border-line bg-surface-2/60 p-3">
        <div class="flex items-center justify-between gap-2">
          <h3 class="text-[12.5px] font-medium">{t('knowledgeSubmit')}</h3>
          <Select
            bind:value={kind}
            testid="knowledge-kind"
            onChange={() => (payload = JSON.stringify(templates[kind], null, 2))}
            options={Object.keys(templates).map((k) => ({ value: k, label: k, mono: true }))}
          />
        </div>
        <textarea placeholder={t('knowledgePayload')} bind:value={payload} rows="8" class="font-mono text-[12px]" data-testid="knowledge-payload"></textarea>
        <p class="text-[11.5px] text-faint">{t('knowledgePermanent')}</p>
        <button class={buttonClass('primary', 'md', 'justify-self-end')} disabled={busy || !agent || !service || !payload.trim()} onclick={submit} data-testid="knowledge-submit">{t('submitTo')}</button>
      </div>
    </div>
  </Section>
{/if}
