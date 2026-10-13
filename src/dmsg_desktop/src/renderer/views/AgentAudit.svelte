<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code, when } from '../rpc'
  import Select from '../components/Select.svelte'

  /** The device's audit record: readable while locked, IDs only. */
  let { agents }: { agents: any[] } = $props()
  let entries = $state<any[]>([])
  let actor = $state('')
  let more = $state(false)
  let error = $state('')
  const PAGE = 50

  async function load(before?: number) {
    error = ''
    try {
      const page: any = await call('audit/list', { agent: actor || null, before: before ?? null, limit: PAGE })
      entries = before ? [...entries, ...page.entries] : page.entries
      more = page.entries.length === PAGE
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['agent/changed', 'approval/changed'].includes(event.type)) void load()
    })
  })

  const labelOf = (who: string) => (who === 'owner' ? t('owner') : (agents.find((a) => a.agent_id === who)?.label ?? who))
</script>

<div class="card" data-testid="agent-audit">
  <div class="row spread">
    <h2>{t('audit')}</h2>
    <Select
      bind:value={actor}
      onChange={() => load()}
      testid="audit-actor"
      options={[
        { value: '', label: t('everyone') },
        { value: 'owner', label: t('owner') },
        ...agents.map((a) => ({ value: a.agent_id, label: a.label }))
      ]} />
  </div>
  <p class="muted">{t('auditHint')}</p>
  {#if error}<p class="error">{error}</p>{/if}
  <ul class="list" data-testid="audit-entries">
    {#each entries as e (e.seq)}
      <li>
        <div class="row spread">
          <span>
            <strong>{e.action}</strong>
            <span class="badge">{e.outcome}</span>
            <span class="muted">· {labelOf(e.actor)} · {e.via}</span>
          </span>
          <span class="muted">{when(e.at)}</span>
        </div>
        {#if e.target || e.detail}<div class="mono muted">{[e.target, e.detail].filter(Boolean).join(' · ')}</div>{/if}
      </li>
    {:else}
      <li class="muted">{t('noAudit')}</li>
    {/each}
  </ul>
  {#if more}<button onclick={() => load(entries[entries.length - 1].seq)}>{t('older')}</button>{/if}
</div>
