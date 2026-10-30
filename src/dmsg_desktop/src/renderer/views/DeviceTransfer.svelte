<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Progress from '../components/Progress.svelte'
  import TextField from '../components/TextField.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { copyText } from '../components/clipboard'
  import { buttonClass } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { bridge, call, errorText, onChange, size, when } from '../rpc'

  /**
   * The file library moving between two devices of the account on the same
   * network: this device receives and shows the session code the sender
   * enters, or sends to a receiving session. One transfer runs at a time;
   * a handover's transfer settles its content step. A stopped transfer
   * resumes in the same session.
   */
  let view = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let sending = $state(false)
  let session = $state('')
  let to = $state('')
  let address = $state('')
  let succession = $state<string | null>(null)
  let pending = $state<{ id: string; params: Record<string, unknown> } | null>(null)
  let alive = true
  onDestroy(() => { alive = false })

  const SESSION = /^[0-9a-f]{32}$/
  const DEVICE = /^[0-9a-f]{64}$/
  const active = $derived(view?.state === 'waiting' || view?.state === 'running')
  const stopped = $derived(view?.state === 'failed' || view?.state === 'cancelled')
  const sessionShown = $derived(session.trim().toLowerCase())
  const toShown = $derived(to.trim().toLowerCase())
  const canSend = $derived(SESSION.test(sessionShown) && (!!succession || DEVICE.test(toShown)))
  const STATES: Record<string, { label: Label; tone: 'neutral' | 'forest' | 'ok' | 'warn' | 'danger' }> = {
    waiting: { label: 'deviceTransferWaiting', tone: 'neutral' },
    running: { label: 'deviceTransferRunning', tone: 'forest' },
    done: { label: 'deviceTransferDone', tone: 'ok' },
    failed: { label: 'deviceTransferFailed', tone: 'danger' },
    cancelled: { label: 'deviceTransferCancelled', tone: 'warn' }
  }

  async function load() {
    try {
      const next = await call('transfer/status')
      if (alive) view = next
    } catch (e) {
      if (alive) error = errorText(e)
    }
  }

  async function attempt(method: string, params: Record<string, unknown> = {}): Promise<boolean> {
    if (busy) return false
    busy = true
    error = ''
    try {
      const next: any = await call(method, params)
      if (!alive) return false
      if (next?.approval) {
        pending = { id: next.approval.id, params }
        await bridge(() => window.dmsg.openApproval(next.approval.id))
        return false
      }
      view = next
      return true
    } catch (e) {
      if (alive) error = errorText(e)
      return false
    } finally {
      if (alive) { busy = false; if (pending) void approved() }
    }
  }

  /** Receive into this device; for handover `task`, its content step. A running transfer stays as it is. */
  export function receive(task?: any) {
    if (active) return
    void attempt('transfer/receive_start', task ? { succession: task.id } : {})
  }

  /** Ask for the receiving session to send to; for handover `task`, to its new device. */
  export function send(task?: any) {
    if (active) return
    session = ''
    address = ''
    to = task?.successor ?? ''
    succession = task?.id ?? null
    error = ''
    pending = null
    sending = true
  }

  async function start() {
    if (!canSend) return
    const params: Record<string, unknown> = { session: sessionShown, ...(succession ? { succession } : { to: toShown }) }
    if (address.trim()) params.address = address.trim()
    if (await attempt('transfer/send_start', params)) sending = false
  }

  async function approved() {
    const request = pending
    if (!request || busy || !alive) return
    try {
      const view: any = await call('approval/show', { id: request.id })
      if (!alive || pending !== request) return
      if (view.state === 'done') {
        pending = null
        if (await attempt('transfer/send_start', { ...request.params, approval: request.id })) sending = false
      } else if (view.state !== 'awaiting_user' && view.state !== 'executing') pending = null
    } catch (e) { if (alive) error = errorText(e) }
  }

  /** The same session again: listening once more, or sending to it once more. */
  function resume() {
    if (!view || active) return
    const handover = view.succession ? { succession: view.succession } : {}
    if (view.role === 'receive') void attempt('transfer/receive_start', { session: view.session, ...handover })
    else void attempt('transfer/send_start', { session: view.session, ...(view.succession ? handover : { to: view.to }), ...(view.addresses?.[0] ? { address: view.addresses[0] } : {}) })
  }

  onMount(() => {
    void load()
    const transfer = onChange(['transfer/changed'], () => void load())
    const session = onChange(['session/changed'], () => { pending = null; void load() })
    const approvals = onChange(['approval/changed'], () => void approved())
    return () => { transfer(); session(); approvals() }
  })
</script>

<Section title={t('deviceTransferTitle')} description={t('deviceTransferHint')} icon="transfer" testid="device-transfer">
  {#snippet actions()}
    <button class={buttonClass('secondary', 'sm')} disabled={busy || active} onclick={() => receive()} data-testid="transfer-receive"><Icon name="download" />{t('deviceTransferReceive')}</button>
    <button class={buttonClass('secondary', 'sm')} disabled={busy || active} onclick={() => send()} data-testid="transfer-send"><Icon name="send" />{t('deviceTransferSend')}</button>
  {/snippet}
  <div class="grid gap-3">
    {#if error && !sending}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if pending && !sending}<Notice>{t('experience.awaiting')}</Notice>{/if}
    {#if view}
      <div class="flex flex-wrap items-center gap-1.5" data-testid="transfer-state">
        <Pill>{t(view.role === 'receive' ? 'deviceTransferReceiving' : 'deviceTransferSending')}</Pill>
        <Pill tone={STATES[view.state]?.tone ?? 'neutral'}>{STATES[view.state] ? t(STATES[view.state].label) : view.state}</Pill>
        {#if view.succession}<Pill tone="forest">{t('successionTitle')}</Pill>{/if}
        <span class="text-[12px] text-faint">{when(view.started_at)}</span>
      </div>
      {#if view.role === 'receive' && active}
        <dl>
          <Fact label={t('deviceTransferSession')}>
            <bdi dir="ltr" class="font-mono text-[13px] tracking-wide select-all" data-testid="transfer-session-code">{view.session}</bdi>
            <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(view.session)}><Icon name="copy" />{t('copy')}</button>
          </Fact>
          <Fact label={t('device')}><Id id={view.to} full wrap /></Fact>
        </dl>
        {#if view.addresses?.length}
          <Notice tone={view.bonjour ? 'warn' : 'info'} icon="relay" testid="transfer-addresses">
            {t(view.bonjour ? 'deviceTransferNoBonjour' : 'deviceTransferManual')}
            <bdi dir="ltr" class="ms-1 font-mono select-all">{view.addresses.join(', ')}</bdi>
          </Notice>
        {/if}
      {/if}
      {#if view.state === 'waiting'}
        <Progress label={t('deviceTransferWaiting')} testid="transfer-progress" />
      {:else if view.state === 'running'}
        <Progress value={view.report.bytes} max={Math.max(1, view.report.total_bytes)} label={t('deviceTransferProgress', { done: size(view.report.bytes), total: size(view.report.total_bytes) })} testid="transfer-progress" />
      {/if}
      {#if view.state === 'done' || (stopped && view.report?.peer)}
        <p class="text-[12.5px] text-muted" data-testid="transfer-counts">
          {t('deviceTransferCounts', { records: view.report.records, kept: view.report.kept, skipped: view.report.skipped, size: size(view.report.bytes) })}
        </p>
      {/if}
      {#if view.error}<Notice failure={view.error.message} tone="danger">{view.error.message}</Notice>{/if}
      {#if active && view.report?.last_error}<p class="text-[12px] text-warn">{view.report.last_error}</p>{/if}
      {#if active || stopped}
        <div class="flex flex-wrap gap-2">
          {#if active}<button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt('transfer/cancel')} data-testid="transfer-cancel">{t('cancel')}</button>{/if}
          {#if stopped}<button class={buttonClass('primary', 'sm')} disabled={busy} onclick={resume} data-testid="transfer-resume"><Icon name="sync" />{t('resume')}</button>{/if}
        </div>
      {/if}
    {:else}
      <p class="text-[12.5px] text-faint">{t('deviceTransferNone')}</p>
    {/if}
  </div>
</Section>

<Dialog bind:open={sending} title={t('deviceTransferSend')} description={t('deviceTransferSendHint')} icon="send" dismissible={!busy} testid="transfer-send-dialog">
  <div class="grid gap-3">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if pending}<Notice>{t('experience.awaiting')}</Notice>{/if}
    <TextField label={t('deviceTransferSession')} bind:value={session} mono disabled={busy} oninput={() => (pending = null)} error={session.trim() && !SESSION.test(sessionShown) ? t('deviceTransferSessionHint') : ''} data-testid="transfer-session" />
    {#if !succession}
      <TextField label={t('deviceTransferTo')} bind:value={to} mono disabled={busy} oninput={() => (pending = null)} error={to.trim() && !DEVICE.test(toShown) ? t('securityDeviceIdHint') : ''} data-testid="transfer-to" />
    {/if}
    <TextField label={t('deviceTransferAddress')} hint={t('deviceTransferAddressHint')} bind:value={address} mono disabled={busy} oninput={() => (pending = null)} data-testid="transfer-address" />
  </div>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => { pending = null; sending = false }}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !canSend || !!pending} aria-busy={busy} onclick={start} data-testid="transfer-start">{t('deviceTransferStart')}</button>
  {/snippet}
</Dialog>
