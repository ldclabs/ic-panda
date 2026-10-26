<script lang="ts">
  import Panel from './Panel.svelte'
  import Select from '../../components/Select.svelte'
  import Field from '../../components/page/Field.svelte'
  import { panel } from './workflow-state.svelte'
  import { card } from './model'
  import { call, when } from '../../rpc'
  import { titleOf, summaryOf } from '../../approval'
  import { buttonClass } from '../../components/ui'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { workflowText as w } from './workflow-text'
  import { ux } from './experience-text'
  import { openVault } from './navigation.svelte'
  let { open, target = '' }: { open: (id: string) => void; target?: string } =
    $props()
  let records = $state<any[]>([]),
    query = $state(''),
    kind = $state('all'),
    statusFilter = $state('all'),
    period = $state('all'),
    page = $state(0)
  $effect(() => {
    query = target
    page = 0
  })
  const model = panel(() =>
    model.request('activity', 'approval/list', { all: true }, (r: any[]) => {
      const anchor = visible[0]?.id
      records = r
        .filter((v) => String(v.type).startsWith('secret_'))
        .sort((a, b) => b.created_at - a.created_at)
      if (page && anchor) {
        const at = filtered.findIndex((r) => r.id === anchor)
        if (at >= 0) page = Math.floor(at / 30)
      }
    })
  )
  const filtered = $derived(
    records.filter(
      (r) =>
        (kind === 'all' || r.type === kind) &&
        (statusFilter === 'all' || r.state === statusFilter) &&
        (period === 'all' ||
          r.created_at >= Date.now() - Number(period) * 86400000) &&
        (!query ||
          JSON.stringify([
            r.id,
            r.detail?.policy_id,
            r.detail?.id,
            summaryOf(r),
            t(titleOf(r).label),
            r.source
          ])
            .toLowerCase()
            .includes(query.toLowerCase()))
    )
  )
  const visible = $derived(filtered.slice(page * 30, (page + 1) * 30))
  const types = $derived([...new Set(records.map((r) => r.type))])
  const states = $derived([...new Set(records.map((r) => r.state))])
</script>

<Panel {model} title={w('activity')} description={w('activityHint')}>
  <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
    <Field label={ux('search')}
      ><input
        bind:value={query}
        type="search"
        oninput={() => (page = 0)}
      /></Field
    ><Field label={ux('kind')}
      ><Select
        bind:value={kind}
        options={[
          { value: 'all', label: ux('all') },
          ...types.map((value) => ({
            value,
            label: t(titleOf({ type: value }).label)
          }))
        ]}
        label={ux('kind')}
        onChange={() => (page = 0)}
        class="w-full"
      /></Field
    ><Field label={ux('status')}
      ><Select
        bind:value={statusFilter}
        options={[
          { value: 'all', label: ux('all') },
          ...states.map((value) => ({ value, label: stateLabel(value) }))
        ]}
        label={ux('status')}
        onChange={() => (page = 0)}
        class="w-full"
      /></Field
    ><Field label={ux('period')}
      ><Select
        bind:value={period}
        options={[
          { value: 'all', label: ux('all') },
          { value: '7', label: ux('week') },
          { value: '30', label: ux('month') }
        ]}
        label={ux('period')}
        onChange={() => (page = 0)}
        class="w-full"
      /></Field
    >
  </div>
  {#each visible as request}<article class={card}>
      <div class="flex flex-wrap justify-between gap-2">
        <h3 class="font-medium">{t(titleOf(request).label)}</h3>
        <span class="text-sm">{stateLabel(request.state)}</span>
      </div>
      <p class="break-words text-sm">{summaryOf(request)}</p>
      <p class="text-xs text-muted">
        {when(request.updated_at ?? request.created_at)}
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          class={buttonClass('secondary')}
          onclick={() => open(request.id)}>{w('review')}</button
        >{#if request.detail?.resolved?.reference?.item_id}<button
            class={buttonClass('ghost')}
            onclick={() => openVault(request.detail.resolved.reference.item_id)}
            >{ux('openSource')}</button
          >{/if}{#if request.state === 'awaiting_user'}<button
            class={buttonClass('secondary')}
            disabled={model.state.busy}
            onclick={() =>
              model.act(() => call('approval/cancel', { id: request.id }))}
            >{t('cancel')}</button
          >{/if}
      </div>
    </article>{:else}{#if model.state.loaded && !model.state.errors.activity}<p
        class="py-8 text-center text-sm text-muted"
      >
        {query || kind !== 'all' || statusFilter !== 'all'
          ? ux('noMatches')
          : ux('empty')}
      </p>{/if}{/each}
  {#if filtered.length > 30}<div class="flex gap-2">
      <button
        class={buttonClass('secondary')}
        disabled={page === 0}
        onclick={() => page--}>{ux('previous')}</button
      ><span class="self-center text-sm"
        >{page + 1} / {Math.ceil(filtered.length / 30)}</span
      ><button
        class={buttonClass('secondary')}
        disabled={(page + 1) * 30 >= filtered.length}
        onclick={() => page++}>{ux('next')}</button
      >
    </div>{/if}
</Panel>
