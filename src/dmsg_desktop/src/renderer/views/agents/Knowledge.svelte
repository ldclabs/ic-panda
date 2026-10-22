<script lang="ts">
  import { untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, code, when } from '../../rpc'

  /** Inspect public contributions at one service; authoring belongs to the Agent. */
  let {
    agents,
    active,
    hosts,
    unlocked,
    manage,
    selectAgent
  }: {
    agents: any[]
    active: string | null
    hosts: any[]
    unlocked: boolean
    manage: () => void
    selectAgent: (id: string) => void
  } = $props()

  const PAGE = 20
  let agent = $state('')
  let service = $state('')
  let mode = $state('knowledge.publish')
  let text = $state('')
  let items = $state<any[] | null>(null)
  let cursor = $state<string | null>(null)
  let scope = $state<any>(null)
  let lookup = $state('')
  let found = $state<any>(null)
  let lifecycle = $state<any>(null)
  let assessments = $state<any[]>([])
  let error = $state('')
  let busy = $state(false)
  const kinds: Record<string, Label> = {
    question: 'knowledgeKindQuestion',
    hypothesis: 'knowledgeKindHypothesis',
    definition: 'knowledgeKindDefinition',
    observation: 'knowledgeKindObservation',
    inference: 'knowledgeKindInference',
    procedure: 'knowledgeKindProcedure',
    resource: 'knowledgeKindResource',
    negative_result: 'knowledgeKindNegative',
    synthesis: 'knowledgeKindSynthesis',
    collection: 'knowledgeKindCollection'
  }
  const titleOf = (item: any) => item.payload?.title ?? item.payload?.summary ?? item.payload?.reason ?? item.type
  const kindOf = (item: any) =>
    item.type === 'knowledge.assess'
      ? t('knowledgeAssessmentsTab')
      : item.type === 'knowledge.retract'
        ? t('knowledgeRetractionsTab')
        : kinds[item.payload?.kind]
          ? t(kinds[item.payload.kind])
          : item.payload?.kind
  const labelOf = (did: string) => agents.find((a) => a.agent_id === did)?.label ?? did

  $effect(() => {
    if (!agent && agents.length) agent = active ?? agents[0].agent_id
  })
  $effect(() => {
    if (!service && hosts.length) service = (hosts.find((h) => /knowledge/i.test(`${h.label ?? ''} ${h.host}`)) ?? hosts[0]).host
  })
  $effect(() => {
    if (unlocked && agent && service && mode)
      untrack(() => {
        items = null
        cursor = null
        found = null
        lifecycle = null
        scope = null
        lookup = ''
        error = ''
        if (mode !== 'search') void look()
      })
  })

  async function tool(name: string, input: Record<string, unknown> = {}) {
    const answer: any = await call('ap/call', { agent, tool: `agent_protocols_${name}`, input })
    if (answer.error) throw Object.assign(new Error(answer.error.message), { tool: answer.error.code })
    return answer.result
  }

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
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
      if (!more) {
        items = null
        cursor = null
        found = null
        lifecycle = null
        scope = null
      }
      const result =
        mode === 'search'
          ? await tool('knowledge_search', { service, text: text.trim(), mode: 'hybrid', limit: PAGE })
          : await tool('knowledge_query', { service, actor: agent, type: mode, limit: PAGE, ...(more && cursor ? { cursor } : {}) })
      const page = mode === 'search' ? result.hits.map((hit: any) => ({ ...hit.item, explanation: hit.explanation })) : result.items
      items = more ? [...(items ?? []), ...page] : page
      cursor = result.next_cursor ?? null
      scope = result
    })

  const get = (id: string) =>
    attempt(async () => {
      found = null
      lifecycle = null
      assessments = []
      // Status already returns the verified event, together with its service-scoped lifecycle.
      const result = await tool('knowledge_status', { service, id })
      found = result.item
      lifecycle = result
      if (result.assessments.length) assessments = (await tool('knowledge_get', { service, ids: result.assessments.slice(0, 100) })).items
    })
</script>

{#if !unlocked || !agents.length}
  <Notice>{unlocked ? t('noAgents') : t('locked')}</Notice>
{:else}
  <Section title={t('knowledge')} description={t('knowledgeHint')} icon="knowledge" testid="agent-knowledge">
    {#snippet actions()}
      <Select
        bind:value={agent}
        onChange={selectAgent}
        disabled={busy}
        label={t('tabAgentIds')}
        testid="knowledge-agent"
        options={agents.map((a) => ({ value: a.agent_id, label: a.label }))}
      />
    {/snippet}
    <div class="grid gap-4">
      <div class="flex items-start gap-3 rounded-xl bg-surface-2 px-4 py-3">
        <Icon name="website" class="mt-0.5 shrink-0 text-forest" />
        <div class="text-[12.5px] leading-relaxed text-muted">
          <p class="font-medium text-ink">{t('knowledgeExplained')}</p>
          <p class="mt-1">{t('knowledgeOwnerRole')}</p>
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-[12px] text-muted">{t('knowledgeService')}</span>
        <Select
          bind:value={service}
          disabled={busy}
          label={t('knowledgeService')}
          placeholder={t('knowledgeNoService')}
          testid="knowledge-service"
          options={hosts.map((h) => ({ value: h.host, label: h.host, mono: true }))}
        />
        <span class="flex-1"></span>
        <button class={buttonClass('ghost', 'sm')} disabled={busy || !service || (mode === 'search' && !text.trim())} onclick={() => look()} data-testid="knowledge-reload"
          ><Icon name="sync-now" />{t('reload')}</button
        >
      </div>
      <div class="overflow-x-auto">
        <Segmented
          bind:value={mode}
          disabled={busy}
          label={t('knowledge')}
          testid="knowledge-mode"
          options={[
            { value: 'knowledge.publish', label: t('knowledgePublicationsTab') },
            { value: 'knowledge.assess', label: t('knowledgeAssessmentsTab') },
            { value: 'knowledge.retract', label: t('knowledgeRetractionsTab') },
            { value: 'search', label: t('knowledgeSearchService') }
          ]}
        />
      </div>
      {#if mode === 'search'}
        <form
          class="flex gap-2"
          onsubmit={(event) => {
            event.preventDefault()
            if (!busy && service && text.trim()) void look()
          }}
        >
          <input
            class="min-w-0 flex-1"
            aria-label={t('knowledgeSearchText')}
            placeholder={t('knowledgeSearchText')}
            bind:value={text}
            disabled={busy}
            data-testid="knowledge-text"
          />
          <button class={buttonClass('secondary')} disabled={busy || !service || !text.trim()} data-testid="knowledge-look"><Icon name="search" />{t('knowledgeSearch')}</button>
        </form>
        <p class="text-[11.5px] text-faint">{t('rankingNote')}</p>
      {:else}<p class="text-[12px] text-muted">{t('knowledgeAgentScope', { agent: labelOf(agent) })}</p>{/if}
      {#if error}<Notice tone="danger" testid="knowledge-error">{error}</Notice>{/if}
      {#if items?.length || found}
        <div class="grid min-h-96 overflow-hidden rounded-xl border border-line lg:grid-cols-[minmax(220px,0.85fr)_minmax(0,1.5fr)]">
          <div class="min-w-0 border-b border-line lg:border-e lg:border-b-0">
            <ul class="max-h-[640px] divide-y divide-line overflow-y-auto" data-testid="knowledge-items">
              {#each items ?? [] as item (item.id)}
                <li>
                  <button
                    class={cn(
                      'grid w-full gap-2 px-4 py-3 text-start outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-forest',
                      found?.id === item.id && 'bg-pale'
                    )}
                    disabled={busy}
                    aria-pressed={found?.id === item.id}
                    onclick={() => get(item.id)}
                  >
                    <span class="text-[11px] text-forest">{kindOf(item)}</span>
                    <span class="line-clamp-2 text-[13px] leading-relaxed font-medium [overflow-wrap:anywhere]">{titleOf(item)}</span>
                    <span class="truncate text-[11.5px] text-muted">{labelOf(item.actor)}</span>
                    <span class="text-[11px] text-faint">{when(item.created_at)}</span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if cursor}<button class={buttonClass('ghost', 'sm', 'm-2')} disabled={busy} onclick={() => look(true)} data-testid="knowledge-more">{t('agentLoadMore')}</button>{/if}
          </div>
          <div class="min-w-0 p-5" data-testid="knowledge-detail">
            {#if found}
              <article class="grid gap-4" data-testid="knowledge-found">
                <div class="flex flex-wrap gap-2">
                  <Pill tone="forest">{kindOf(found)}</Pill><Pill tone={lifecycle?.status === 'retracted' ? 'warn' : 'neutral'}
                    >{lifecycle?.complete ? (lifecycle.status === 'retracted' ? t('knowledgeRetracted') : t('knowledgeActive')) : t('knowledgeStatusIncomplete')}</Pill
                  >
                </div>
                <h3 class="text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]">{titleOf(found)}</h3>
                <div class="flex flex-wrap items-center gap-2 text-[11.5px] text-muted"><Id id={found.actor} /><span>{when(found.created_at)}</span></div>
                {#if found.payload.statement}<p class="text-[13px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{found.payload.statement}</p>{/if}
                {#if found.payload.target}<div class="flex flex-wrap items-center gap-2 text-[12px]">
                    <span class="text-muted">{t('knowledgeTarget')}</span><button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => get(found.payload.target)}
                      >{t('open')}</button
                    ><Id id={found.payload.target} />
                  </div>{/if}
                {#if found.payload.verdict}<p class="text-[12px]">{t('knowledgeVerdict')}: {found.payload.verdict}</p>{/if}
                {#if found.payload.context}
                  <div class="grid gap-3 rounded-lg bg-surface-2 p-3 text-[12.5px] leading-relaxed">
                    <div>
                      <h4 class="mb-1 font-medium">{t('knowledgeScope')}</h4>
                      <p class="whitespace-pre-wrap [overflow-wrap:anywhere]">{found.payload.context.scope}</p>
                    </div>
                    {#each [{ key: 'conditions', label: t('knowledgeConditions') }, { key: 'limitations', label: t('knowledgeLimitations') }] as field}
                      <div>
                        <h4 class="mb-1 font-medium">{field.label}</h4>
                        {#if found.payload.context[field.key]?.length}<ul class="list-disc space-y-1 ps-4">
                            {#each found.payload.context[field.key] as entry}<li class="[overflow-wrap:anywhere]">{entry}</li>{/each}
                          </ul>{:else}<p class="text-muted">{t('knowledgeNotStated')}</p>{/if}
                      </div>
                    {/each}
                  </div>
                {/if}
                {#if found.payload.basis}<div>
                    <h4 class="mb-1 text-[12px] font-medium">{t('knowledgeBasis')}</h4>
                    <p class="text-[12.5px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{found.payload.basis}</p>
                  </div>{/if}
                {#if found.payload.evidence?.length}
                  <div class="grid gap-2">
                    <h4 class="text-[12px] font-medium">{t('knowledgeEvidence')}</h4>
                    {#each found.payload.evidence as evidence}<div class="grid gap-1 border-s-2 border-line ps-3 text-[12px]">
                        <p class="[overflow-wrap:anywhere]">{evidence.description}</p>
                        <Id id={evidence.url} full wrap /><span class="text-faint">{t('knowledgeEvidenceUnchecked')}</span>
                      </div>{/each}
                  </div>
                {/if}
                {#if found.payload.relations?.length}
                  <details class="text-[12px]">
                    <summary class="cursor-pointer">{t('knowledgeRelations')} · {found.payload.relations.length}</summary>
                    <ul class="mt-2 grid gap-2">
                      {#each found.payload.relations as relation}<li class="flex flex-wrap items-center gap-2">
                          <span>{relation.relation}</span><Id id={relation.target} /><button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => get(relation.target)}
                            >{t('open')}</button
                          >
                        </li>{/each}
                    </ul>
                  </details>
                {/if}
                <div class="grid gap-2 border-t border-line pt-3">
                  <h4 class="text-[12px] font-medium">{t('assessments', { n: lifecycle?.assessments.length ?? 0 })}</h4>
                  {#each assessments as assessment (assessment.id)}
                    <button
                      class="grid gap-1 rounded-lg bg-surface-2 p-3 text-start text-[12px] outline-none hover:bg-wash focus-visible:ring-2 focus-visible:ring-forest"
                      disabled={busy}
                      onclick={() => get(assessment.id)}
                      ><span class="font-medium [overflow-wrap:anywhere]">{assessment.payload.summary}</span><span class="truncate text-muted"
                        >{labelOf(assessment.actor)} · {assessment.payload.verdict}</span
                      ></button
                    >
                  {:else}<p class="text-[12px] text-muted">{t('knowledgeNoAssessments')}</p>{/each}
                  {#if lifecycle?.assessments.length > assessments.length}<p class="text-[11.5px] text-muted">{t('knowledgeAssessmentLimit')}</p>{/if}
                </div>
                {#each [{ ids: lifecycle?.successors ?? [], label: t('knowledgeSuccessors') }, { ids: lifecycle?.retractions ?? [], label: t('knowledgeRetractionsTab') }] as group}
                  {#if group.ids.length}<div class="grid gap-2">
                      <h4 class="text-[12px] font-medium">{group.label}</h4>
                      {#each group.ids as id}<div class="flex items-center gap-2">
                          <Id {id} /><button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => get(id)}>{t('open')}</button>
                        </div>{/each}
                    </div>{/if}
                {/each}
                <p class="text-[11.5px] leading-relaxed text-faint">{t('knowledgeStatusScope')} {lifecycle?.service} · {when(lifecycle?.as_of)}</p>
                <details class="text-[11.5px] text-muted">
                  <summary class="cursor-pointer">{t('agentTechnicalDetails')}</summary>
                  <div class="mt-2 grid gap-2">
                    <Id id={found.id} full wrap />{#if found.payload.license}<Id id={found.payload.license} full wrap />{/if}
                    <pre class="max-h-80 overflow-auto rounded-lg bg-surface-2 p-3 font-mono whitespace-pre-wrap [overflow-wrap:anywhere]">{JSON.stringify(
                        found.payload,
                        null,
                        2
                      )}</pre>
                  </div>
                </details>
              </article>
            {:else}<div class="grid min-h-80 place-content-center gap-3 text-center">
                <Icon name="knowledge" class="mx-auto size-7 text-faint" />
                <p class="text-[13px] text-muted">{busy ? t('loading') : t('knowledgeSelect')}</p>
              </div>{/if}
          </div>
        </div>
      {:else}
        <div class="grid min-h-60 place-content-center justify-items-center gap-3 rounded-xl border border-dashed border-line px-8 text-center" data-testid="knowledge-empty">
          <Icon name="knowledge" class="size-8 text-faint" />
          <h3 class="text-[14px] font-medium">
            {busy ? t('loading') : error ? t('agentLoadFailed') : !service ? t('knowledgeNoService') : mode === 'search' && !items ? t('knowledgeSearchText') : t('knowledgeEmpty')}
          </h3>
          <p class="max-w-md text-[12.5px] leading-relaxed text-muted">{t('knowledgeEmptyHint')}</p>
        </div>
      {/if}
      {#if scope}<p class="text-[11.5px] text-faint [overflow-wrap:anywhere]">
          {t('knowledgeStatusScope')}
          {scope.service} · {when(scope.as_of)} · checkpoint {scope.checkpoint}
        </p>{/if}
      <details class="text-[12px] text-muted">
        <summary class="cursor-pointer">{t('knowledgeLookup')}</summary>
        <form
          class="mt-2 flex gap-2"
          onsubmit={(event) => {
            event.preventDefault()
            if (!busy && service && lookup.trim()) void get(lookup.trim())
          }}
        >
          <input aria-label={t('knowledgeLookup')} bind:value={lookup} class="min-w-0 flex-1 font-mono" disabled={busy} /><button
            class={buttonClass('secondary')}
            disabled={busy || !service || !lookup.trim()}>{t('open')}</button
          >
        </form>
      </details>
      <div class="flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
        <span class="text-[12px] text-muted">{t('agentPermissionsHint')}</span><button class={buttonClass('ghost', 'sm')} onclick={manage}
          ><Icon name="security" />{t('tabCapabilities')}</button
        >
      </div>
    </div>
  </Section>
{/if}
