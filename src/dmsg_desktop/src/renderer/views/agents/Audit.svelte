<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { call, errorText, onChange, single, when } from '../../rpc'
  import { days } from '../channels/channels'
  import { outcomeTone } from './agents'

  /** The device's audit record as a timeline: who did what, through which way in. Readable while locked; IDs only. */
  let { agents }: { agents: any[] } = $props()

  let entries = $state<any[]>([])
  let actor = $state('')
  let more = $state(false)
  let error = $state('')
  let alive = true
  let revision = 0
  let displayedActor = ''
  onDestroy(() => { alive = false; revision++ })
  const PAGE = 50

  async function load(before?: number) {
    const turn = ++revision
    const selected = actor
    if (displayedActor !== actor) { entries = []; more = false; displayedActor = actor }
    error = ''
    try {
      const page: any = await call('audit/list', { agent: actor || null, before: before ?? null, limit: PAGE })
      if (!alive || turn !== revision || actor !== selected) return
      entries = before ? [...entries, ...page.entries] : page.entries
      more = page.entries.length === PAGE
    } catch (e) {
      if (!alive || turn !== revision || actor !== selected) return
      error = errorText(e)
    }
  }

  /** The first page again, once per burst of changes. */
  const reload = single(() => load())

  onMount(() => {
    void reload()
    return onChange(['agent/changed', 'approval/changed', 'audit/changed'], reload)
  })

  const labelOf = (who: string) => (who === 'owner' ? t('owner') : (agents.find((a) => a.agent_id === who)?.label ?? who))
  const groups = $derived(days(entries.map((e) => ({ ...e, stored_at: e.at }))).reverse())
  const dayFormat = $derived(new Intl.DateTimeFormat(locale(), { weekday: 'short', month: 'short', day: 'numeric' }))
  const timeFormat = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))
  const dots = { ok: 'bg-ok', danger: 'bg-danger', warn: 'bg-warn' }
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
  {#if error}<Notice tone="danger" class="mb-3">{error}</Notice>{/if}
  <div data-testid="audit-entries">
    {#each groups as group (group.day)}
      <h3 class="sticky top-0 bg-surface py-1.5 text-[11.5px] font-medium text-faint">{dayFormat.format(group.at)}</h3>
      <ol class="relative ms-1.5 border-s border-line ps-5">
        {#each [...group.messages].reverse() as e (e.seq)}
          {@const tone = outcomeTone(e.outcome)}
          <li class="relative pb-3.5">
            <span class={cn('absolute -start-[25px] top-1.5 size-2.5 rounded-full ring-3 ring-surface', dots[tone])}></span>
            <div class="flex flex-wrap items-center gap-1.5">
              <b class="font-mono text-[12.5px] font-medium">{e.action}</b>
              <Pill tone={tone}>{e.outcome}</Pill>
              <span class="text-[12px] text-muted">{labelOf(e.actor)}</span>
              <Pill>{e.via}</Pill>
              <time class="ms-auto text-[11.5px] text-faint" title={when(e.at)}>{timeFormat.format(e.at)}</time>
            </div>
            {#if e.target || e.detail}
              <p class="mt-0.5 font-mono text-[11.5px] break-all text-faint">{[e.target, e.detail].filter(Boolean).join(' · ')}</p>
            {/if}
          </li>
        {/each}
      </ol>
    {:else}
      <p class="text-[13px] text-faint">{t('noAudit')}</p>
    {/each}
  </div>
  {#if more}
    <button class={buttonClass('secondary', 'sm', 'mt-2')} onclick={() => load(entries[entries.length - 1].seq)}>{t('older')}</button>
  {/if}
</Section>
