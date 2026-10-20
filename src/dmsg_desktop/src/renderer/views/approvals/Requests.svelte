<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { stateLabel, t } from '../../i18n/index.svelte'
  import { ago, when } from '../../rpc'
  import { kindIcon, kindOf, reasonLabels, summaryOf, titleOf, trailOf } from '../../approval'

  /**
   * The requests: those waiting for a decision, each opened in the approval
   * window or withdrawn when this device's own clients made it; those
   * approved and still running, reconciled now on request; and the decided
   * ones, filtered by kind and opened read-only.
   */
  let {
    requests,
    agents,
    busy,
    open,
    cancel,
    reconcile
  }: {
    requests: any[]
    agents: Record<string, string>
    busy: boolean
    open: (id: string) => unknown
    cancel: (id: string) => unknown
    reconcile: () => unknown
  } = $props()

  let kind = $state('all')
  let now = $state(Date.now())

  $effect(() => {
    const clock = setInterval(() => (now = Date.now()), 15_000)
    return () => clearInterval(clock)
  })

  const waiting = $derived(requests.filter((r) => r.state === 'awaiting_user'))
  const running = $derived(requests.filter((r) => r.state === 'executing' || r.state === 'unknown'))
  const decided = $derived(
    requests
      .filter((r) => !['awaiting_user', 'executing', 'unknown'].includes(r.state) && (kind === 'all' || kindOf(r) === kind))
      // The latest decision first; the list shows when each was decided.
      .sort((a, b) => (b.decided_at ?? b.updated_at) - (a.decided_at ?? a.updated_at))
  )

  const title = (r: any) => {
    const found = titleOf(r)
    return found.detail ? `${t(found.label)}: ${found.detail}` : t(found.label)
  }
  const source = (r: any) => trailOf(r, agents)[0]
  const reason = (code: string) => (reasonLabels[code] ? t(reasonLabels[code]) : code)
  const stateTone = (state: string) => (state === 'done' ? 'ok' : ['rejected', 'failed', 'expired', 'lost'].includes(state) ? 'danger' : 'neutral')
  const minutes = (ms: number) => Math.max(1, Math.ceil(ms / 60_000))

  const kinds = $derived([
    { value: 'all', label: t('allKinds') },
    ...['kindSignature', 'kindWeb', 'kindPayment', 'kindSecurity', 'kindAgent', 'kindWallet', 'kindName'].map((k) => ({ value: k, label: t(k as any) }))
  ])
</script>

<Section title={t('waiting')} icon="pending" flush>
  <ul class="divide-y divide-line border-t border-line" data-testid="waiting-list">
    {#each waiting as request (request.id)}
      {@const k = kindOf(request)}
      {@const from = source(request)}
      <li class="flex items-center gap-3 px-5 py-3">
        <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-gem-pale text-gem"><Icon name={kindIcon[k] ?? 'info'} /></span>
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <b class="text-[13px] font-medium">{title(request)}</b>
            {#if request.risk === 'high'}<Pill tone="warn">{t('highRisk')}</Pill>{/if}
            <Pill><Icon name={from.icon} class="size-3" />{from.name || t(from.role)}</Pill>
            <span class="text-[11.5px] text-faint" title={when(request.created_at)}>{ago(request.created_at)}</span>
            <span class={cn('text-[11.5px] tabular-nums', request.expires_at - now <= 60_000 ? 'text-warn' : 'text-faint')} data-testid="waiting-left">
              {t('minutesLeft', { n: minutes(request.expires_at - now) })}
            </span>
          </div>
          <p class="truncate text-[12.5px] text-muted">{summaryOf(request)}</p>
          {#if request.error}<p class="text-[12px] text-warn" data-testid="waiting-error">{t('notRunYet')} {reason(request.error)}</p>{/if}
        </div>
        {#if request.type !== 'connect'}
          <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => cancel(request.id)} data-testid="request-cancel">{t('withdraw')}</button>
        {/if}
        <button class={buttonClass('primary', 'sm')} onclick={() => open(request.id)} data-testid="request-open">{t('open')}</button>
      </li>
    {:else}
      <li class="flex items-center gap-2 px-5 py-4 text-[13px] text-faint"><Icon name="check" class="text-ok" />{t('noRequests')}</li>
    {/each}
  </ul>
</Section>

{#if running.length}
  <Section title={t('inProgress')} description={t('inProgressHint')} icon="sync" flush testid="running-list">
    {#snippet actions()}
      <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={reconcile} data-testid="reconcile"><Icon name="sync-now" />{t('reconcileNow')}</button>
    {/snippet}
    <ul class="divide-y divide-line border-t border-line">
      {#each running as request (request.id)}
        <li class="flex items-center gap-3 px-5 py-2.5">
          <Pill tone="warn">{stateLabel(request.state)}</Pill>
          <button class="min-w-0 flex-1 truncate text-start text-[12.5px] hover:underline" onclick={() => open(request.id)}>{title(request)} · {summaryOf(request)}</button>
          {#if request.error}<span class="shrink-0 text-[11.5px] text-faint">{reason(request.error)}</span>{/if}
        </li>
      {/each}
    </ul>
  </Section>
{/if}

<Section title={t('history')} icon="time" flush>
  {#snippet actions()}
    <Select bind:value={kind} options={kinds} testid="history-kind" />
  {/snippet}
  <ul class="divide-y divide-line border-t border-line" data-testid="history-list">
    {#each decided as request (request.id)}
      <li>
        <button class="flex w-full items-center gap-3 px-5 py-2.5 text-start hover:bg-wash" onclick={() => open(request.id)} data-testid="history-open">
          <Pill tone={stateTone(request.state)}>{stateLabel(request.state)}</Pill>
          <span class="min-w-0 flex-1">
            <span class="block truncate text-[12.5px]">{title(request)} · {summaryOf(request)}</span>
            {#if ['failed', 'lost'].includes(request.state) && request.error}<span class="block text-[11.5px] text-faint">{reason(request.error)}</span>{/if}
          </span>
          <span class="shrink-0 text-[11.5px] text-faint" title={when(request.decided_at ?? request.updated_at)}>{ago(request.decided_at ?? request.updated_at)}</span>
        </button>
      </li>
    {:else}
      <li class="px-5 py-3 text-[13px] text-faint">-</li>
    {/each}
  </ul>
</Section>
