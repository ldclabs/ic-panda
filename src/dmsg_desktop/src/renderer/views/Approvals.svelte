<script lang="ts">
  import { onMount } from 'svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { toast } from '../components/toast.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { stateLabel, t, type Label } from '../i18n/index.svelte'
  import { ago, bridge, call, errorText, onChange, single, when } from '../rpc'
  import { kindOf } from '../approval'

  /**
   * Signatures and approvals in parts: the requests waiting and decided, new
   * statements and application actions with the month's allowance, and
   * attestation bundles exported and verified.
   */
  let { status }: { status: any } = $props()

  type Tab = 'requests' | 'sign' | 'bundles'

  let tab = $state<Tab>('requests')
  let requests = $state<any[]>([])
  let history = $state<any[]>([])
  let usage = $state<any>(null)
  let error = $state('')
  let text = $state('')
  let report = $state<any>(null)
  let busy = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const waiting = $derived(requests.filter((r) => r.state === 'awaiting_user'))
  const decided = $derived(requests.filter((r) => r.state !== 'awaiting_user'))

  const kindIcon: Record<Label, Glyph> = {
    kindWeb: 'website',
    kindPayment: 'coins',
    kindSignature: 'signature',
    kindName: 'user',
    kindAgent: 'agents',
    kindSecurity: 'security',
    kindOther: 'info'
  } as Record<Label, Glyph>

  const reload = single(async () => {
    error = ''
    try {
      ;[requests, history] = await Promise.all([call('approval/list', { all: true }), call('attest/history')])
    } catch (e) {
      requests = []
      history = []
      error = errorText(e)
    }
  })

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      toast(() => errorText(e))
    } finally {
      busy = false
    }
  }

  const open = (id: string) => attempt(() => bridge(() => window.dmsg.openApproval(id)))

  const sign = () =>
    attempt(async () => {
      if (!text.trim()) return
      const request: any = await call('attest/request', { statement: { profile: 'text', text } })
      text = ''
      await bridge(() => window.dmsg.openApproval(request.id))
    })

  /** Refreshes the month's allowance from the plan and archives finished months here. */
  const loadUsage = () =>
    attempt(async () => {
      usage = await call('attest/usage')
    })

  const enableFormal = () =>
    attempt(async () => {
      const request: any = await call('device/formal_approve', { enable: true })
      await bridge(() => window.dmsg.openApproval(request.id))
    })

  /** An action its application prepared, approved in the approval window like a page's `signAction`. */
  const signAction = () =>
    attempt(async () => {
      const request: any = await bridge(() => window.dmsg.attestAction())
      if (request) await bridge(() => window.dmsg.openApproval(request.id))
    })

  /** Files are picked in the main process's dialogs; the window sees only the outcome. */
  const verify = (withFile: boolean) =>
    attempt(async () => {
      report = (await bridge(() => window.dmsg.verifyBundle(withFile))) ?? report
    })

  const exportBundle = (execution: string) =>
    attempt(async () => {
      const saved = await bridge<{ path: string } | null>(() => window.dmsg.exportBundle(execution))
      if (saved) toast(() => `${t('bundleSaved')}: ${saved.path}`, { testid: 'approvals-notice' })
    })

  /** What a statement says, in one line. */
  const summary = (s: any) => (s.profile === 'app_action' ? `${s.content.app_id} · ${s.content.command}` : (s.content.text ?? s.content.sha256))

  const label = (r: any) =>
    r.type === 'connect' ? `${r.detail.app_id} · ${r.detail.origin}` : r.type === 'attest' ? `${r.detail.statement.profile}: ${summary(r.detail.statement)}` : r.type

  const stateTone = (state: string) => (state === 'done' ? 'ok' : state === 'rejected' || state === 'failed' || state === 'expired' ? 'danger' : 'neutral')

  $effect(() => {
    if (tab === 'sign' && unlocked && !usage) void loadUsage()
  })

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed'], reload)
  })

  const tabs = $derived([
    { value: 'requests', label: t('requests'), icon: 'approvals' as const, count: waiting.length },
    { value: 'sign', label: t('newStatement'), icon: 'signature' as const },
    { value: 'bundles', label: t('bundles'), icon: 'verified' as const }
  ])
</script>

<div class="flex h-full min-h-0 flex-col">
  <TabBar bind:value={() => tab, (next) => (tab = next as Tab)} items={tabs} label={t('approvals')} testid="approvals-tab" />
  <div class="min-h-0 flex-1">
    <Page>
      {#if error}<Notice tone="danger" testid="approvals-error">{error}</Notice>{/if}

      {#if tab === 'requests'}
        <Section title={t('waiting')} icon="pending" flush>
          <ul class="divide-y divide-line border-t border-line" data-testid="waiting-list">
            {#each waiting as request (request.id)}
              {@const kind = kindOf(request)}
              <li class="flex items-center gap-3 px-5 py-3">
                <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-gem-pale text-gem"><Icon name={kindIcon[kind] ?? 'info'} /></span>
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <b class="text-[13px] font-medium">{t(kind)}</b>
                    <Pill>{request.source}</Pill>
                    <span class="text-[11.5px] text-faint">{ago(request.created_at)}</span>
                  </div>
                  <p class="truncate text-[12.5px] text-muted">{label(request)}</p>
                </div>
                <button class={buttonClass('primary', 'sm')} onclick={() => open(request.id)}>{t('open')}</button>
              </li>
            {:else}
              <li class="flex items-center gap-2 px-5 py-4 text-[13px] text-faint"><Icon name="check" class="text-ok" />{t('noRequests')}</li>
            {/each}
          </ul>
        </Section>
        <Section title={t('history')} icon="time" flush>
          <ul class="divide-y divide-line border-t border-line">
            {#each decided as request (request.id)}
              <li class="flex items-center gap-3 px-5 py-2.5">
                <Pill tone={stateTone(request.state)}>{stateLabel(request.state)}</Pill>
                <span class="min-w-0 flex-1 truncate text-[12.5px]">{label(request)}</span>
                <span class="shrink-0 text-[11.5px] text-faint" title={when(request.created_at)}>{ago(request.created_at)}</span>
              </li>
            {:else}
              <li class="px-5 py-3 text-[13px] text-faint">-</li>
            {/each}
          </ul>
        </Section>
      {:else if tab === 'sign'}
        {#if unlocked}
          <Section title={t('newStatement')} description={t('statementHint')} icon="signature">
            <div class="grid gap-2">
              <textarea placeholder={t('statement')} bind:value={text} rows="4" data-testid="statement-text"></textarea>
              <div class="flex justify-end">
                <button class={buttonClass('primary')} disabled={busy || !text.trim()} onclick={sign} data-testid="statement-submit">{t('submit')}</button>
              </div>
            </div>
          </Section>
          <Section title={t('appAction')} description={t('appActionHint')} icon="api">
            <button class={buttonClass('secondary')} disabled={busy} data-testid="sign-action" onclick={signAction}><Icon name="upload" />{t('appActionPick')}</button>
          </Section>
          <Section title={t('usage')} description={t('usageHint')} icon="plan">
            {#snippet actions()}
              <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={loadUsage}><Icon name="sync-now" />{t('refresh')}</button>
            {/snippet}
            {#if usage}
              {@const share = usage.allowed ? Math.min(1, usage.charged / usage.allowed) : 0}
              <div class="grid gap-2" data-testid="usage">
                <div class="flex items-baseline justify-between gap-2">
                  <span class="text-[22px] font-semibold tabular-nums">{usage.charged}<span class="text-[14px] font-normal text-faint"> / {usage.allowed}</span></span>
                  <span class="text-[12px] text-muted">{usage.remaining} {t('remaining')} · {t('validUntil')} {when(usage.valid_until_ms)}</span>
                </div>
                <div class="h-2 overflow-hidden rounded-full bg-wash">
                  <div class={cn('h-full rounded-full transition-[width] duration-500', share > 0.9 ? 'bg-warn' : 'bg-forest')} style:width="{share * 100}%"></div>
                </div>
                {#if usage.history.length}
                  <ul class="mt-2 grid gap-1 text-[12px]">
                    {#each usage.history as month (month.month)}
                      <li class="flex justify-between text-muted"><span>{month.month}</span><span class="tabular-nums">{month.charged} / {month.allowed}</span></li>
                    {/each}
                  </ul>
                {/if}
              </div>
            {:else}
              <p class="shimmer text-[13px]">{t('loading')}</p>
            {/if}
          </Section>
          <Section title={t('formal')} description={t('formalHint')} icon="verified">
            <button class={buttonClass('secondary')} disabled={busy} onclick={enableFormal}>{t('formal')}: {t('enable')}</button>
          </Section>
        {:else}
          <Notice>{t('locked')}</Notice>
        {/if}
      {:else}
        <Section title={t('verifyBundle')} description={t('verifyBundleHint')} icon="verified" testid="verify-bundle">
          <div class="flex flex-wrap gap-2">
            <button class={buttonClass('secondary')} disabled={busy} onclick={() => verify(false)}><Icon name="upload" />{t('verifyBundle')}</button>
            <button class={buttonClass('secondary')} disabled={busy} onclick={() => verify(true)}><Icon name="file" />{t('verifyWithFile')}</button>
          </div>
          {#if report}
            <div class="mt-4 overflow-hidden rounded-xl border border-line" data-testid="verify-report">
              <div class={cn('flex items-center gap-3 px-4 py-3', report.signature ? 'bg-ok-pale' : 'bg-danger-pale')}>
                <span class={cn('grid size-9 place-items-center rounded-full text-paper', report.signature ? 'bg-ok' : 'bg-danger')}>
                  <Icon name={report.signature ? 'check' : 'close'} />
                </span>
                <div class="min-w-0">
                  <b class="block text-[14px] font-semibold">{report.signature ? t('reportValid') : t('reportInvalid')}</b>
                  <span class="text-[12px] text-muted">{t('content')}: {report.content}</span>
                </div>
              </div>
              <dl class="divide-y divide-line px-4">
                <Fact label={t('signature')}>{report.signature ? t('valid') : t('invalid')} <Id id={report.key_fingerprint} /></Fact>
                <Fact label={t('account')}><Id id={report.account} full /></Fact>
                <Fact label={t('device')}><Id id={report.device_id} /> <Pill>{report.current}</Pill></Fact>
                <Fact label={t('approvedAt')}>{when(report.approved_at)} · {t('securityEpoch')} {report.security_epoch}</Fact>
                <Fact label={t('origin')}><bdi dir="ltr" class="font-mono text-[12.5px]">{report.origin}</bdi></Fact>
                <Fact label={t('certifiedAt')}>{when(report.certified_at)}</Fact>
                <Fact label={t('statement')}>{report.statement?.profile}: {summary(report.statement)}</Fact>
              </dl>
            </div>
          {/if}
        </Section>
        <Section title={t('signed')} icon="signature" flush>
          <ul class="divide-y divide-line border-t border-line">
            {#each history as entry (entry.execution)}
              <li class="flex items-start gap-3 px-5 py-3">
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <Pill tone={entry.stage === 'complete' ? 'ok' : 'neutral'}>{entry.stage}</Pill>
                    <span class="text-[11.5px] text-faint">{when(entry.created_at)}</span>
                  </div>
                  <p class="mt-1 text-[13px] [overflow-wrap:anywhere]">{summary(entry.statement)}</p>
                  <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[11.5px] text-faint">
                    <bdi dir="ltr" class="font-mono">{entry.origin}</bdi><Id id={entry.execution} />
                  </div>
                </div>
                {#if entry.stage === 'complete'}
                  <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => exportBundle(entry.execution)}><Icon name="download" />{t('exportBundle')}</button>
                {/if}
              </li>
            {:else}
              <li class="px-5 py-3 text-[13px] text-faint">-</li>
            {/each}
          </ul>
        </Section>
      {/if}
    </Page>
  </div>
</div>
