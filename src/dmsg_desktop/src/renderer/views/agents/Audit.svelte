<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Empty from '../../components/page/Empty.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, stateLabel, t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'
  import { days } from '../channels/channels'
  import { outcomeTone } from './agents'

  /**
   * The device's audit record as a timeline. Each entry reads as one line,
   * who did what and how it ended; the recorded fields stay one click away.
   * Readable while locked; IDs only.
   */
  let { agents }: { agents: any[] } = $props()

  let entries = $state<any[]>([])
  let loaded = $state(false)
  let busy = $state(false)
  let failedBefore = $state<number | undefined>()
  let actor = $state('')
  let more = $state(false)
  let error = $state('')
  let alive = true
  let revision = 0
  let refreshPending = false
  let displayedActor = ''
  onDestroy(() => { alive = false; revision++ })
  const PAGE = 50

  async function load(before?: number, preserve = false) {
    const turn = ++revision
    const selected = actor
    if (displayedActor !== actor) { entries = []; more = false; loaded = false; displayedActor = actor }
    error = ''; busy = true; failedBefore = undefined
    try {
      const page: any = await call('audit/list', { agent: actor || null, before: before ?? null, limit: PAGE })
      if (!alive || turn !== revision || actor !== selected) return
      const next = page.entries as any[]
      if (preserve && entries.length > PAGE && next.length && entries.some(e => e.seq === next[next.length - 1].seq)) {
        entries = [...next, ...entries.filter(e => e.seq < next[next.length - 1].seq)]
      } else {
        entries = before === undefined ? next : [...entries, ...next]
        more = next.length === PAGE
      }
      loaded = true
    } catch (e) {
      if (!alive || turn !== revision || actor !== selected) return
      error = errorText(e); failedBefore = before
    } finally {
      if (alive && turn === revision) {
        busy = false
        if (refreshPending) { refreshPending = false; void reload() }
      }
    }
  }

  /** The first page again, once per burst of changes. */
  const reload = single(() => load(undefined, true))

  onMount(() => {
    void reload()
    return onChange(['agent/changed', 'approval/changed', 'audit/changed'], () => {
      if (busy) refreshPending = true
      else void reload()
    })
  })

  const labelOf = (who: string) => (who === 'owner' ? t('owner') : (agents.find((a) => a.agent_id === who)?.label ?? who))
  const groups = $derived(days(entries.map((e) => ({ ...e, stored_at: e.at }))).reverse())
  const dayFormat = $derived(new Intl.DateTimeFormat(locale(), { weekday: 'short', month: 'short', day: 'numeric' }))
  const timeFormat = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))
  const dots = { ok: 'bg-ok', danger: 'bg-danger', warn: 'bg-warn' }
  /** `dmsg_vault_read` and `grants/add` as words: the tool or method the entry names. */
  const deed = (action: string) => action.replace(/^dmsg_/, '').replace(/[_/.]+/g, ' ').trim()
  const ending = { ok: 'done', danger: 'rejected', warn: 'failed' } as const
</script>

<Section title={t('audit')} description={t('auditHint')} icon="audit" testid="agent-audit">
  {#snippet actions()}
    <Select
      bind:value={actor}
      onChange={() => load()}
      testid="audit-actor"
      label={t('audit')}
      options={[{ value: '', label: t('everyone') }, { value: 'owner', label: t('owner') }, ...agents.map((a) => ({ value: a.agent_id, label: a.label }))]}
    />
  {/snippet}
  {#if error}<Notice tone="danger" class="mb-3" testid="audit-error">{error}
    {#snippet action()}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => load(failedBefore)}>{t('retry')}</button>{/snippet}
  </Notice>{/if}
  <div data-testid="audit-entries">
    {#each groups as group (group.day)}
      <h3 class="sticky top-0 bg-surface py-1.5 text-caption font-medium text-faint">{dayFormat.format(group.at)}</h3>
      <ol class="relative ms-1.5 border-s border-line ps-5">
        {#each [...group.messages].reverse() as e (e.seq)}
          {@const tone = outcomeTone(e.outcome)}
          <li class="relative pb-2">
            <span class={cn('absolute -start-[25px] top-2 size-2.5 rounded-full ring-3 ring-surface', dots[tone])}></span>
            <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <p class="min-w-0 flex-1 text-body [overflow-wrap:anywhere]"><b class="font-medium">{labelOf(e.actor)}</b> <span class="text-ink-2">{deed(e.action)}</span></p>
              <Pill tone={tone}>{stateLabel(ending[tone])}</Pill>
              <time class="text-caption text-faint" title={when(e.at)}>{timeFormat.format(e.at)}</time>
            </div>
            <Collapsible testid="audit-fields">
              {#snippet title()}<span class="text-caption text-faint">{t('technical')}</span>{/snippet}
              <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 pb-1 font-mono text-code text-muted select-text">
                {#each [['action', e.action], ['outcome', e.outcome], ['via', e.via], ['actor', e.actor], ['target', e.target], ['detail', e.detail]] as [name, value] (name)}
                  {#if value}<dt class="text-faint">{name}</dt><dd class="break-all">{value}</dd>{/if}
                {/each}
              </dl>
            </Collapsible>
          </li>
        {/each}
      </ol>
    {:else}
      {#if error}{:else if !loaded}<p class="text-body text-muted" role="status">{t('loading')}</p>
      {:else}<div role="status"><Empty icon="audit" title={actor ? t('noResults') : t('noAudit')} class="py-8">
        {#snippet actions()}{#if actor}<button class={buttonClass('secondary', 'sm')} onclick={() => { actor = ''; void load() }}>{t('everyone')}</button>{/if}{/snippet}
      </Empty></div>{/if}
    {/each}
  </div>
  {#if more && !error}
    <button disabled={busy} class={buttonClass('secondary', 'sm', 'mt-2')} onclick={() => load(entries[entries.length - 1].seq)}>{t('older')}</button>
  {/if}
</Section>
