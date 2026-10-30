<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Confirm from '../components/Confirm.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import TextArea from '../components/TextArea.svelte'
  import TextField from '../components/TextField.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, size, when } from '../rpc'
  import { walletRoute } from './wallet/model'

  /**
   * Handing a device over to a new one. The old device reviews the new
   * device's request and approves it; then each side runs its steps in
   * order. Steps that ask open the approval window; local content moves
   * with a device transfer or an import from a copied workspace, and it and
   * native addresses can be left behind, which the task records.
   */
  let { go, onTransfer }: { go?: (path: string) => void; onTransfer: (role: 'receive' | 'send', task: any) => void } = $props()

  const STEPS: Record<string, { label: Label; hint: Label }> = {
    authorize: { label: 'successionStepAuthorize', hint: 'successionStepAuthorizeHint' },
    approval_key: { label: 'successionStepApprovalKey', hint: 'successionStepApprovalKeyHint' },
    cosigners: { label: 'successionStepCosigners', hint: 'successionStepCosignersHint' },
    content: { label: 'successionStepContent', hint: 'successionStepContentHint' },
    wallets: { label: 'successionStepWallets', hint: 'successionStepWalletsHint' },
    native_addresses: { label: 'successionStepNative', hint: 'successionStepNativeHint' },
    revoke: { label: 'successionStepRevoke', hint: 'successionStepRevokeHint' }
  }
  const DEVICE = /^[0-9a-f]{64}$/

  let tasks = $state<any[]>([])
  let copies = $state<any[]>([])
  let error = $state('')
  let notice = $state('')
  let busy = $state(false)
  let loaded = $state(false)
  let reviewing = $state(false)
  let token = $state('')
  let review = $state<any>(null)
  let reviewError = $state('')
  let shown = $state('')
  let local = $state<any>(null)
  let found = $state<Record<string, any[]>>({})
  let leaving = $state<{ task: any; step: string; items: string[] } | null>(null)
  let stopping = $state<any>(null)
  let revoking = $state<any>(null)
  let alive = true
  onDestroy(() => { alive = false })

  const shownId = $derived(shown.trim().toLowerCase())
  const matches = $derived(!!review && shownId === review.device_id)

  /** A step as this device sees it: done, left behind, ready to run here, or waiting. */
  function stepState(task: any, step: any): 'done' | 'left' | 'ready' | 'waiting' {
    if (step.settled) return step.evidence === null ? 'left' : 'done'
    return !task.cancelled_at && task.available.includes(step.step) ? 'ready' : 'waiting'
  }

  /** What a task left behind: its left steps with the items recorded for them. */
  function leftBehind(task: any): string[] {
    return task.steps
      .filter((s: any) => stepState(task, s) === 'left')
      .flatMap((s: any) => (s.skipped.length ? s.skipped.map((item: string) => (item === 'library' ? t('files') : item)) : [t(STEPS[s.step].label)]))
  }

  const finished = (task: any) => task.side === 'successor' && task.steps.every((s: any) => s.settled)
  const copiesOf = (task: any) => copies.filter((c) => c.device_id === task.predecessor && c.account === task.account)

  async function load() {
    try {
      const [list, moved] = await Promise.all([call<any[]>('device/successions'), call<any[]>('device/moved_list').catch(() => [])])
      if (!alive) return
      tasks = [...(list ?? [])].reverse()
      copies = moved ?? []
      error = ''
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) loaded = true
    }
  }

  async function attempt(work: () => Promise<void>) {
    if (busy || !alive) return
    busy = true
    error = ''
    notice = ''
    try {
      await work()
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      if (alive) busy = false
      await load()
    }
  }

  const ask = (id: string) => bridge(() => window.dmsg.openApproval(id))

  function startReview() {
    token = ''
    shown = ''
    review = null
    reviewError = ''
    notice = ''
    reviewing = true
    void call('files/local_only').then((value) => { if (alive) local = value }, () => {})
  }

  async function check() {
    if (busy || !token.trim()) return
    busy = true
    reviewError = ''
    review = null
    try {
      const value = await call('device/succession_review', { token: token.trim() })
      if (alive) review = value
    } catch (e) {
      if (alive) reviewError = errorText(e)
    } finally {
      if (alive) busy = false
    }
  }

  async function approve() {
    if (busy || !matches) return
    busy = true
    reviewError = ''
    try {
      const request: any = await call('device/succession_approve', { token: token.trim(), device: shownId })
      if (!alive) return
      if (Array.isArray(request.requests)) {
        notice = `${t('successionApprovals', { n: request.requests.length })} ${t('successionRefreshRequest')}`
        review = null
        token = ''
        if (request.requests[0]?.id) await ask(request.requests[0].id)
        return
      }
      reviewing = false
      await ask(request.id)
    } catch (e) {
      if (alive) reviewError = errorText(e)
    } finally {
      if (alive) busy = false
      await load()
    }
  }

  /** Run `step` here: an approval to decide, wallet approvals, addresses to move, or the step recorded. */
  const run = (task: any, step: string) =>
    attempt(async () => {
      const result: any = await call('device/succession_run', { id: task.id, step })
      if (!alive) return
      if (result?.kind && result?.id) await ask(result.id)
      else if (Array.isArray(result?.requests)) {
        if (result.requests[0]?.id) await ask(result.requests[0].id)
        notice = t('successionApprovals', { n: result.requests.length })
      } else if (Array.isArray(result?.addresses)) found[task.id] = result.addresses
      else if (Array.isArray(result?.rotations)) notice = t('successionPartial')
    })

  const importCopy = (task: any, copy: any) =>
    attempt(async () => {
      const report: any = await call('device/moved_import', { id: copy.id, succession: task.id })
      if (alive) notice = t('movedImported', { records: report.records, size: size(report.bytes) })
    })

  function leave(task: any, step: string) {
    const items = step === 'content' ? ['library'] : (found[task.id] ?? []).map((a: any) => `${a.wallet} #${a.from}`)
    leaving = { task, step, items }
  }

  const skip = () => {
    const chosen = leaving
    if (!chosen) return
    return attempt(async () => {
      await call('device/succession_skip', { id: chosen.task.id, step: chosen.step, skipped: chosen.items })
      if (alive) leaving = null
    })
  }

  const stop = () =>
    attempt(async () => {
      await call('device/succession_cancel', { id: stopping.id })
      if (alive) stopping = null
    })

  const revoke = () => {
    const task = revoking
    revoking = null
    if (task) void run(task, 'revoke')
  }

  onMount(() => {
    void load()
    return onChange(['approval/changed', 'session/changed', 'transfer/changed', 'wallet/changed'], () => void load())
  })
</script>

<Section title={t('successionTitle')} description={t('successionHint')} icon="transfer" flush testid="succession">
  {#snippet actions()}
    <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={startReview} data-testid="succession-review"><Icon name="paste" />{t('successionReview')}</button>
  {/snippet}
  {#if error}<div class="px-5 pb-3"><Notice failure={error} tone="danger">{error}</Notice></div>{/if}
  {#if notice}<div class="px-5 pb-3"><Notice tone="ok" testid="succession-notice">{notice}
    {#snippet action()}{#if go}<button class={buttonClass('secondary', 'xs')} onclick={() => go('/approvals')}>{t('approvals')}</button>{/if}{/snippet}
  </Notice></div>{/if}
  {#if !loaded}<p class="px-5 pb-3 text-[12.5px] text-muted" role="status">{t('loading')}</p>{/if}
  <ul class="divide-y divide-line">
    {#each tasks as task (task.id)}
      <li class={cn('grid gap-2 px-5 py-3.5', task.cancelled_at && 'opacity-70')} data-testid="succession-task">
        <div class="flex flex-wrap items-center gap-1.5">
          <span class="text-[13px] font-medium">{t(task.side === 'successor' ? 'successionReplacing' : 'successionReplacedBy')}</span>
          <Id id={task.side === 'successor' ? task.predecessor : task.successor} />
          {#if task.cancelled_at}<Pill tone="warn">{t('successionStopped')}</Pill>{/if}
          <span class="min-w-0 flex-1 text-[12px] text-faint">{when(task.created_at)}</span>
          {#if !task.cancelled_at && !finished(task)}
            <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => (stopping = task)}>{t('successionStop')}</button>
          {/if}
        </div>
        <ol class="grid gap-1.5">
          {#each task.steps as s (s.step)}
            {@const shownState = stepState(task, s)}
            <li class={cn('rounded-xl border px-3 py-2', shownState === 'ready' ? 'border-forest/35 bg-pale/60' : 'border-line')} data-testid={`succession-step-${s.step}`}>
              <div class="flex flex-wrap items-center gap-2">
                <Icon name={shownState === 'done' ? 'check' : shownState === 'left' ? 'leave' : shownState === 'ready' ? 'enter' : 'pending'} class={cn('size-4 shrink-0', shownState === 'done' ? 'text-ok' : shownState === 'left' ? 'text-warn' : shownState === 'ready' ? 'text-forest' : 'text-faint')} />
                <span class="min-w-0 flex-1 text-[13px]">{t(STEPS[s.step].label)}</span>
                <Pill tone={shownState === 'done' ? 'ok' : shownState === 'left' ? 'warn' : shownState === 'ready' ? 'forest' : 'neutral'}>
                  {shownState === 'done' ? t('done') : shownState === 'left' ? t('successionLeft') : shownState === 'ready' ? t('successionReady') : t('successionWaiting')}
                </Pill>
              </div>
              {#if shownState === 'ready'}
                <p class="mt-1 text-[12px] text-muted">{t(STEPS[s.step].hint)}</p>
                <div class="mt-2 flex flex-wrap gap-1.5">
                  {#if s.step === 'content'}
                    {#if task.side === 'successor'}
                      <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => onTransfer('receive', task)} data-testid="succession-receive">{t('deviceTransferReceive')}</button>
                      {#each copiesOf(task) as copy (copy.id)}
                        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => importCopy(task, copy)} data-testid="succession-import">{t('successionImport')}</button>
                      {/each}
                    {:else}
                      <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => onTransfer('send', task)} data-testid="succession-send">{t('deviceTransferSend')}</button>
                    {/if}
                    <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => leave(task, 'content')}>{t('successionLeave')}</button>
                  {:else if s.step === 'native_addresses'}
                    <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => run(task, s.step)} data-testid="succession-native">{t('successionCheck')}</button>
                    <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => leave(task, s.step)}>{t('successionLeave')}</button>
                  {:else if s.step === 'revoke'}
                    <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => (revoking = task)} data-testid="succession-revoke">{t('successionStepRevoke')}</button>
                  {:else}
                    <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => run(task, s.step)} data-testid={`succession-run-${s.step}`}>
                      {s.step === 'authorize' ? t('approve') : s.step === 'cosigners' && task.side === 'successor' ? t('successionConfirm') : t('successionRun')}
                    </button>
                  {/if}
                </div>
                {#if s.step === 'native_addresses' && found[task.id]?.length}
                  <ul class="mt-2 grid gap-1" data-testid="succession-addresses">
                    {#each found[task.id] as a (`${a.wallet}:${a.from}`)}
                      <li class="flex flex-wrap items-center gap-2 text-[12px]">
                        <Id id={a.wallet} /><span class="text-faint">#{a.from}</span>
                        {#if go}<button class={buttonClass('secondary', 'xs')} onclick={() => go(`${walletRoute(a.wallet)}?move=${a.from}`)}>{t('wallet.nativeMove')}</button>{/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              {:else if shownState === 'left' && s.skipped.length}
                <p class="mt-1 text-[12px] break-all text-faint">{s.skipped.map((item: string) => (item === 'library' ? t('files') : item)).join(' · ')}</p>
              {/if}
            </li>
          {/each}
        </ol>
        {#if finished(task)}<Notice tone="ok" icon="check">{t('successionFinished')}</Notice>{/if}
      </li>
    {:else}
      {#if loaded && !error}<li class="px-5 py-3 text-[12.5px] text-faint">{t('successionNone')}</li>{/if}
    {/each}
  </ul>
</Section>

<Dialog bind:open={reviewing} title={t('successionReview')} description={t('successionReviewHint')} icon="device" dismissible={!busy} wide testid="succession-review-dialog">
  <div class="grid gap-3">
    {#if reviewError}<Notice failure={reviewError} tone="danger">{reviewError}</Notice>{/if}
    {#if notice}<Notice>{notice}</Notice>{/if}
    <TextArea label={t('successionToken')} bind:value={token} rows={3} mono disabled={busy} oninput={() => (review = null)} data-testid="succession-token" />
    {#if review}
      <dl data-testid="succession-reviewed">
        <Fact label={t('device')}><Id id={review.device_id} full wrap /></Fact>
        <Fact label={t('role')}>{review.role}</Fact>
        <Fact label={t('capabilities')}>{#each review.capabilities as capability (capability)}<Pill tone="ok" class="font-normal">{capability}</Pill>{/each}</Fact>
        <Fact label={t('successionKeys')}>
          <Pill tone={review.approval_key ? 'ok' : 'neutral'}>{t('successionApprovalKey')}: {review.approval_key ? t('successionMade') : t('successionNotMade')}</Pill>
          <Pill tone={review.cosigner ? 'ok' : 'neutral'}>{t('successionCosigner')}: {review.cosigner ? t('successionMade') : t('successionNotMade')}</Pill>
        </Fact>
        <Fact label={t('successionWallets')}>{review.wallets.length ? review.wallets.map((w: any) => w.name || w.wallet).join(' · ') : '-'}</Fact>
        {#if local}<Fact label={t('successionContent')}>{size(local.bytes)}</Fact>{/if}
        <Fact label={t('successionExpires')}>{when(review.expires_at)}</Fact>
      </dl>
      <Notice tone="warn">{t('successionApproveHint')}</Notice>
      <TextField label={t('successionDevice')} bind:value={shown} mono disabled={busy} error={shownId && DEVICE.test(shownId) && !matches ? t('successionMismatch') : shownId && !DEVICE.test(shownId) ? t('securityDeviceIdHint') : ''} data-testid="succession-device" />
    {/if}
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => (reviewing = false)}>{t('cancel')}</button>
    {#if review}
      <button class={buttonClass('primary')} disabled={busy || !matches} aria-busy={busy} onclick={approve} data-testid="succession-approve">{t('successionApprove')}</button>
    {:else}
      <button class={buttonClass('primary')} disabled={busy || !token.trim()} aria-busy={busy} onclick={check} data-testid="succession-check">{t('successionCheckRequest')}</button>
    {/if}
  {/snippet}
</Dialog>

<Confirm
  bind:open={() => !!leaving, (open) => { if (!open) leaving = null }}
  title={t('successionLeaveTitle')}
  description={t(leaving?.step === 'content' ? 'successionLeaveContent' : 'successionLeaveNative')}
  confirm={t('successionLeave')}
  icon="leave"
  tone="warn"
  {busy}
  onConfirm={skip}
  testid="succession-leave"
/>
<Confirm
  bind:open={() => !!stopping, (open) => { if (!open) stopping = null }}
  title={t('successionStop')}
  description={t('successionStopBody')}
  confirm={t('successionStop')}
  icon="close"
  tone="warn"
  {busy}
  onConfirm={stop}
  testid="succession-stop"
/>
<Confirm
  bind:open={() => !!revoking, (open) => { if (!open) revoking = null }}
  title={t('successionStepRevoke')}
  description={t('successionRevokeBody')}
  confirm={t('revoke')}
  icon="device"
  {busy}
  onConfirm={revoke}
  testid="succession-revoke-dialog"
>
  {#snippet body()}
    {#if revoking && leftBehind(revoking).length}
      <p class="text-[12.5px] font-medium">{t('successionLeft')}</p>
      <ul class="mt-1 list-disc ps-5 text-[12.5px] break-all text-muted">{#each leftBehind(revoking) as item (item)}<li>{item}</li>{/each}</ul>
    {/if}
  {/snippet}
</Confirm>
