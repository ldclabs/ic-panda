<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { locale, t, stateLabel, type Label } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, single, size, units, when } from '../rpc'
  import {
    actionValue,
    changeLabels,
    holdToApprove,
    kindOf,
    pickLabel,
    purposeLabels,
    reasonLabels,
    renderedAction,
    statementOf,
    stepsOf,
    titleOf,
    trailOf
  } from '../approval'
  import WalletIntent from './wallet/Intent.svelte'
  import { knownIntent } from './wallet/model'
  import { w } from './wallet/text'
  import Collapsible from '../components/Collapsible.svelte'
  import Icon from '../components/Icon.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { callerLabels, nameActionLabels, skuOf } from './plan/plan'

  /**
   * The approval window, laid out as a ticket: who asks and the way the
   * request came, what exactly is signed, paid or changed, what approving
   * does, and the technical fields folded below. Only this window decides,
   * with the digest of what it shows. Approve opens after a short delay and
   * again whenever the request changes or the window comes back to the
   * front; Reject has the focus; requests the daemon rates high-risk take a
   * one-second hold, and Touch ID or the login password where the Mac can
   * ask. Locked, it shows nothing of the request and offers the unlock.
   */
  let { id }: { id: string } = $props()
  let request = $state<any>(null)
  let status = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let approving = $state(false)
  let assets = $state<any[]>([])
  let selection = $state('')
  let readiness = $state<any>(null)
  let walletAssets = $state<any[]>([])
  let walletReadiness = $state<any>(null)
  let queue = $state<string[]>([])
  let agents = $state<Record<string, string>>({})
  let now = $state(Date.now())
  let ready = $state(false)
  let held = $state(0)
  let rejectButton = $state<HTMLButtonElement>()
  let delay: ReturnType<typeof setTimeout> | undefined
  let frame = 0
  /** This window read the device's state once already. */
  let offered = false
  /** The deadline passed here, and the daemon was asked once. */
  let lapsed = false

  const APPROVE_DELAY_MS = 1200
  const HOLD_MS = 1000
  const inset = window.dmsg.platform === 'darwin'

  /** A request seen for the first time or changed is reviewed afresh: Approve waits again, and a hold under way stops. */
  function arm() {
    ready = false
    stopHold()
    clearTimeout(delay)
    delay = setTimeout(() => (ready = true), APPROVE_DELAY_MS)
  }

  async function load() {
    try {
      const next: any = await call('approval/show', { id })
      if (next?.digest !== request?.digest) arm()
      const first = !request
      const before = request?.state
      request = next
      // A pending execution that settled no longer shows its old notice;
      // a failure's reason is rendered with the outcome below.
      if (before === 'executing' && next?.state !== 'executing') error = ''
      if (first && next?.state === 'awaiting_user') {
        await tick()
        rejectButton?.focus()
      }
      void loadReadiness()
      if (first && next?.type === 'wallet') {
        if (next.detail?.wallet) void call<any>('wallet/assets', { wallet: next.detail.wallet }).then((r) => walletAssets = r.assets, () => undefined)
      }
      void loadQueue()
      // An agent's request, a grant to an agent or its credential names Agent IDs.
      if (first && (String(next?.source).startsWith('agent:') || ['delegation', 'grant'].includes(next?.type))) void loadAgents()
    } catch (e) {
      // Locked: nothing of the request stays on screen.
      if (code(e) === 'LOCKED') request = null
      error = errorText(e)
    }
  }

  const reload = single(load)

  /** Whether signing can happen now: the request's purpose, this device's approval right, the policy and the login. */
  const loadReadiness = single(async () => {
    const current = request
    const signs = current?.type === 'attest' || (current?.type === 'connect' && ['signDocument', 'signAction'].includes(current?.detail?.method))
    readiness = signs && current?.state === 'awaiting_user' ? await call('attest/readiness', { id }).catch(() => null) : null
    walletReadiness = null
    if (current?.type === 'wallet' && current.detail?.action !== 'cosign' && current.state === 'awaiting_user') {
      const result = await call('wallet/readiness').catch(() => ({ blockers: ['WALLET_REGISTRATION_UNAVAILABLE'] }))
      if (request?.id === current.id && request?.state === 'awaiting_user') walletReadiness = result
    }
  })

  /** The other requests waiting, for the way to the next one. */
  const loadQueue = single(async () => {
    const list: any[] = await call('approval/list', {}).catch(() => [])
    queue = list.filter((r) => r.state === 'awaiting_user' && r.id !== id).map((r) => r.id)
  })

  /** The names of the Agent IDs this device hosts. */
  async function loadAgents() {
    const hosted: any = await call('agent/ids').catch(() => null)
    agents = Object.fromEntries((hosted?.agents ?? []).map((a: any) => [a.agent_id, a.label]))
  }

  /** The device's state: locked, the request goes from the window; unlocked again, it comes back. */
  const refreshStatus = single(async () => {
    status = await call('daemon/status').catch(() => null)
    const opening = !offered
    offered = true
    if (status && status.state !== 'unlocked') {
      request = null
      readiness = null
      walletReadiness = null
      stopHold()
      // Opened while locked: ask for the fast unlock once, as the main
      // window does; a lock while it is open asks nothing.
      if (opening && status.state === 'locked' && status.fast_presence) void unlockHere()
    } else if (status && !request) {
      error = ''
      await load()
    }
  })

  async function unlockHere() {
    try {
      await call('session/unlock', { fast: true })
    } catch (e) {
      if (!['FORBIDDEN'].includes(code(e))) error = errorText(e)
    }
  }

  const showMain = (route: string) => bridge(() => window.dmsg.showMain(route)).catch((e) => (error = errorText(e)))

  /** Decide exactly what is shown: the digest of this view goes with the answer. */
  async function decide(approve: boolean) {
    busy = true
    approving = approve
    error = ''
    try {
      request = await bridge(() => window.dmsg.decide(request.id, request.digest, approve))
      void loadQueue()
    } catch (e) {
      const label = reasonLabels[code(e)]
      error = label ? t(label) : `${code(e)}: ${e instanceof Error ? e.message : String(e)}`
      await load()
    } finally {
      busy = false
      approving = false
    }
  }

  /** A web checkout is quoted here first, so the amount approved is the amount shown. */
  async function quote() {
    busy = true
    error = ''
    try {
      request = await call('approval/quote', { id: request.id, selection: selection.trim() })
      arm()
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  /** Compare a file the person has with the digest to sign; what is approved stays the same. */
  async function checkFile() {
    busy = true
    error = ''
    try {
      const checked = await bridge(() => window.dmsg.checkFile(request.id))
      if (checked) request = checked
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }

  /** Turn on formal approval for this device: its own request, decided here first. */
  async function enableFormal() {
    try {
      const created: any = await call('device/formal_approve', { enable: true })
      location.hash = `/approval/${created.id}`
    } catch (e) {
      error = errorText(e)
    }
  }

  function startHold() {
    if (!canApprove || frame) return
    const started = performance.now()
    const step = (time: number) => {
      held = Math.min(1, (time - started) / HOLD_MS)
      if (held < 1) {
        frame = requestAnimationFrame(step)
        return
      }
      frame = 0
      held = 0
      if (canApprove) void decide(true)
    }
    frame = requestAnimationFrame(step)
  }

  function stopHold() {
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    held = 0
  }

  function holdKey(event: KeyboardEvent, down: boolean) {
    if (event.key !== ' ' && event.key !== 'Enter') return
    event.preventDefault()
    if (!down) stopHold()
    else if (!event.repeat) startHold()
  }

  /** Back in front, the request is reviewed afresh: a click meant for another window does not approve. */
  const refocus = () => {
    if (request?.state === 'awaiting_user') {
      arm()
      void loadReadiness()
    }
  }

  onMount(() => {
    void refreshStatus().then(async () => {
      if (request?.detail?.method === 'checkout' && request.detail.content?.method === 'Cash') {
        assets = await call('plan/assets').catch(() => [])
        if (assets.length) selection = assets[0].ledger
      }
    })
    const offs = [onChange(['approval/changed'], reload), onChange(['session/changed'], refreshStatus)]
    const clock = setInterval(() => (now = Date.now()), 1000)
    window.addEventListener('focus', refocus)
    return () => {
      for (const off of offs) off()
      clearInterval(clock)
      clearTimeout(delay)
      stopHold()
      window.removeEventListener('focus', refocus)
    }
  })

  const locked = $derived(!!status && status.state !== 'unlocked')
  const d = $derived(request?.detail ?? {})
  const s = $derived(statementOf(request))
  const rendered = $derived(renderedAction(request))
  /** Terms of a checkout: a web checkout's quote, or this device's own. */
  const terms = $derived(request?.type === 'connect' ? d.checkout : request?.type === 'checkout' ? request.detail : null)
  const unquoted = $derived(request?.type === 'connect' && d.method === 'checkout' && !d.checkout)

  /** The catalog names a storage pack's size; read once a checkout shows. */
  let catalog = $state<any>(null)
  $effect(() => {
    if (request?.type === 'checkout' && !catalog)
      untrack(() => {
        call('plan/catalog').then(
          (read) => (catalog = read),
          () => undefined
        )
      })
  })

  /** What a checkout buys, for people. */
  function bought(view: any) {
    const sku = skuOf(String(view.sku ?? ''), catalog)
    if (sku.kind === 'upgrade') return t('upgradeTo', { plan: sku.plan ?? '' })
    if (sku.kind === 'storage') return sku.bytes != null ? t('storagePackOf', { size: size(sku.bytes) }) : t('storagePack')
    return sku.plan ?? String(view.sku ?? '')
  }

  /** What to know before paying: when a renewal starts, what an upgrade or a pack covers, what a stake commits. */
  function checkoutNotes(view: any): string {
    const sku = skuOf(String(view.sku ?? ''), catalog)
    const notes: string[] = []
    if (sku.kind === 'upgrade') notes.push(t('approvalUpgradeNote', { time: when(view.expires_at_ms) }))
    else if (sku.kind === 'storage') notes.push(t('approvalStorageNote', { time: when(view.expires_at_ms) }))
    else if (view.created_at_ms != null && view.starts_at_ms > view.created_at_ms + 60_000) notes.push(t('approvalRenewalNote', { time: when(view.starts_at_ms) }))
    if (view.terms?.method === 'Panda') notes.push(t('approvalPandaNote', { time: when(view.terms.committed_until_ms) }))
    return notes.join(' ')
  }
  const waiting = $derived(request?.state === 'awaiting_user')
  const left = $derived(request?.expires_at ? Math.max(0, request.expires_at - now) : null)
  const expired = $derived(left === 0)
  /** How long the request had from its creation, for the bar of the time left. */
  const lifetime = $derived(Math.max(1, (request?.expires_at ?? 0) - (request?.created_at ?? (request?.expires_at ?? 0) - 600_000)))
  const hold = $derived(holdToApprove(request))
  const blockers = $derived<string[]>([...(readiness?.blockers ?? []), ...(request?.type === 'wallet' && request?.detail?.action !== 'cosign' ? (walletReadiness?.blockers ?? ['WALLET_REGISTRATION_UNAVAILABLE']) : []), ...(request?.type === 'wallet' && request?.detail?.proposal && !knownIntent(request.detail.proposal.intent) ? ['WALLET_UNSUPPORTED_INTENT'] : [])])
  const canApprove = $derived(waiting && ready && !busy && !unquoted && !expired && !blockers.length)
  const trail = $derived(trailOf(request, agents))
  const kind = $derived(kindOf(request))
  const steps = $derived(stepsOf(request, approving))
  const lang = $derived(locale())

  // The deadline passed while the window was open: the daemon says it expired.
  $effect(() => {
    if (waiting && expired && !lapsed) {
      lapsed = true
      void reload()
    }
  })

  const title = $derived.by(() => {
    if (!request) return ''
    const found = titleOf(request)
    return found.detail ? `${t(found.label)}: ${found.detail}` : t(found.label)
  })

  /** An account security change for people; unknown changes show as they are. */
  const change = (name: string): string => (changeLabels[name] ? t(changeLabels[name]) : name)
  const purpose = (name: string): string => (purposeLabels[name] ? t(purposeLabels[name]) : name)
  const purposes = (names?: string[]) => (names ?? []).map(purpose).join(', ')
  const reason = (codeName: string): string => codeName.startsWith('WALLET_') ? (codeName==='WALLET_UNSUPPORTED_INTENT' ? w('unsupported') : w(codeName)) : (reasonLabels[codeName] ? t(reasonLabels[codeName]) : `${t('error')}: ${codeName}`)
  const authPurposes: Record<string, Label> = { Login: 'authLogin', Link: 'authLink', Reauthenticate: 'authReauthenticate' }
  const method = (name?: string) => (name === 'Cash' ? t('methodCash') : name === 'Panda' ? t('methodPanda') : String(name ?? ''))

  /** Who asks: the web application, the agent, the terminal or this device's main window. */
  const who = $derived(
    request?.type === 'connect' ? { name: String(d.app_id ?? ''), detail: String(d.origin ?? '') } : { name: trail[0]?.name ?? '', detail: t(trail[0]?.role ?? 'sourceOther') }
  )

  /** A web page asks this device to vouch that the account approved the page's session. */
  const authenticating = $derived(request?.type === 'connect' && d.method === 'authenticate')

  const section = $derived<Label>(s || authenticating ? 'youSign' : kind === 'kindPayment' ? 'youPay' : 'youChange')

  /** What approving does, in plain words; the last lines say what cannot be undone. */
  const after = $derived.by((): [string, boolean][] => {
    if (!request || request.detail === null) return []
    if (s)
      return [
        [t('afterSign'), false],
        [request.type === 'connect' ? t('afterSignWeb', { origin: String(d.origin ?? '') }) : t('afterSignRecord'), false],
        [t('afterNoUndoSign'), true]
      ]
    const pay: [string, boolean] = [t('afterPay'), false]
    const final: [string, boolean] = [t('afterNoUndoPay'), true]
    switch (request.type) {
      case 'connect':
        if (authenticating) return [[t('afterAuthenticate', { origin: String(d.origin ?? '') }), false]]
        return d.method === 'checkout' ? [pay, final] : []
      case 'checkout':
        return [pay, final]
      case 'handle':
        if (d.total) return [pay, final]
        return d.action === 'Transfer' ? [[t('afterName'), false], [t('afterNoUndoName'), true]] : [[t('afterName'), false]]
      case 'letter':
        return [pay, [t('afterLetter'), false], final]
      case 'delegation':
        return d.kind?.type === 'revoke'
          ? [
              [t('afterRevokeGrant'), false],
              [t('delegationWarn'), true]
            ]
          : [[t('afterGrant'), false]]
      case 'principal':
        return [[t('afterController'), false]]
      case 'grant':
        return [[t('afterCapabilityGrant'), false]]
      case 'account': {
        const line = ({
          bind_login: t('afterBindLogin'),
          remove_login: t('afterRemoveLogin'),
          policy: t('afterPolicy'),
          recovery_delay: t('afterRecoveryDelay'),
          dispute_recovery: t('afterDispute')
        } as Record<string, string>)[d.change]
        return line ? [[line, false]] : []
      }
      case 'device_capabilities':
        return [[t('afterCapabilities'), false]]
      case 'device_revoke':
        return [
          [t('afterRevokeDevice'), false],
          [t('afterRotate'), false],
          [t('afterCopies'), true]
        ]
      case 'wallet':
        if (d.action === 'create') return [[t('twAfterCreate'), false], final]
        if (d.action === 'join') return [[t('twAfterJoin'), false]]
        if (d.action === 'cosign') return [[t('twAfterCosign'), false], final]
        return [[t('twAfterDecide'), false]]
      default:
        return []
    }
  })

  /** Where the person can fix what stops a signature. */
  const fixes: Record<string, { label: Label; run: () => unknown }> = {
    AUTH_REQUIRED: { label: 'loginInMain', run: () => showMain('/login') },
    FORMAL_APPROVE: { label: 'enableFormalHere', run: enableFormal },
    FROZEN: { label: 'openSecurity', run: () => showMain('/settings/security') },
    PURPOSE: { label: 'openSecurity', run: () => showMain('/settings/security') },
    QUOTA: { label: 'openPlan', run: () => showMain('/plan') }
  }

  const clock = (ms: number) => {
    const seconds = Math.ceil(ms / 1000)
    return seconds >= 6000 ? when(request?.expires_at) : `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
  }
</script>

{#snippet row(label: string, value: string, options: { mono?: boolean; multiline?: boolean; testid?: string } = {})}
  <dt class="text-faint">{label}</dt>
  <dd class={cn('min-w-0 [overflow-wrap:anywhere] text-ink-2', options.mono && 'font-mono text-xs leading-5', options.multiline && 'whitespace-pre-wrap')} data-testid={options.testid}>{value}</dd>
{/snippet}

{#snippet nameRows()}
  {@render row(t('names'), `@${d.name ?? ''}`)}
  {@render row(t('nameAction'), nameActionLabels[d.action] ? t(nameActionLabels[d.action]) : String(d.action ?? ''), { testid: 'approval-name-action' })}
  {@render row(callerLabels[d.caller_role] ? t(callerLabels[d.caller_role]) : t('payer'), String(d.caller ?? ''), { mono: true })}
  {#if d.from}{@render row(t('fromAccount'), d.from, { mono: true })}{/if}
  {#if d.to}{@render row(t('toAccount'), d.to, { mono: true, testid: 'approval-name-to' })}{/if}
  {#if d.total}
    {@render row(t('price'), `${units(d.total, 8)} PANDA`)}
    {@render row(t('networkFee'), `${units(d.fee ?? '0', 8)} PANDA`)}
    {@render row(t('atMost'), `${units(d.most ?? d.total, 8)} PANDA`)}
  {/if}
  {#if d.action === 'Transfer'}
    {@render row(t('note'), t('approvalTransferNote'), { testid: 'approval-note' })}
  {:else if d.action === 'AcceptTransfer'}
    {@render row(t('note'), t('approvalAcceptNote'), { testid: 'approval-note' })}
  {/if}
{/snippet}

{#snippet field(f: any)}
  <dt class="text-faint">{pickLabel(f.label, lang)}</dt>
  <dd class={cn('min-w-0 [overflow-wrap:anywhere] text-ink-2', ['hash', 'principal', 'artifact'].includes(f.value?.type) && 'font-mono text-xs leading-5', f.value?.multiline && 'whitespace-pre-wrap')}>
    {#if f.value?.type === 'record'}
      <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 border-s border-line ps-2.5">
        {#each f.value.fields as inner (inner.name)}{@render field(inner)}{/each}
      </dl>
    {:else}
      {actionValue(f.value, lang)}
    {/if}
  </dd>
{/snippet}

<div class="flex h-screen flex-col overflow-hidden bg-surface text-[13.5px] text-ink">
  {#if inset}
    <div class="h-9 shrink-0 border-b border-line bg-paper-dim text-center text-xs leading-9 text-faint [-webkit-app-region:drag]">{t('approvalWindow')}</div>
  {/if}
  {#if locked}
    <div class="grid flex-1 place-items-center p-6" data-testid="approval-locked">
      <div class="grid max-w-xs justify-items-center gap-3 text-center">
        <span class="grid size-12 place-items-center rounded-2xl border border-line bg-paper text-faint [&_svg]:size-6"><Icon name="lock" /></span>
        <p class="text-[13.5px] text-muted">{t('approvalLocked')}</p>
        {#if status?.state === 'locked' && status.fast_unlock}
          <button class={buttonClass('primary')} onclick={unlockHere} data-testid="approval-unlock">
            <Icon name="unlock" />{t(status.fast_presence === 'biometrics' ? 'touchIdUnlock' : 'fastUnlock')}
          </button>
        {/if}
        <button class={buttonClass('ghost', 'sm')} onclick={() => showMain(status?.state === 'locked' && status.fast_unlock ? '/approvals' : '/login')} data-testid="approval-main">
          {t('openMainToUnlock')}
        </button>
        {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
      </div>
    </div>
  {:else if !request}
    <p class="p-5 text-muted">{error || '…'}</p>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto px-5 pt-4 pb-5">
      <div class="flex items-center gap-2">
        <span class="inline-flex h-6 items-center gap-1.5 rounded-md bg-gem-pale px-2 text-xs font-semibold text-gem">
          <i class="size-[7px] rotate-45 rounded-[1px] bg-current"></i>{t(kind)}
        </span>
        {#if request.risk === 'high'}<span class="inline-flex h-6 items-center rounded-md bg-warn-pale px-2 text-xs font-medium text-warn" data-testid="approval-risk">{t('highRisk')}</span>{/if}
        {#if queue.length}<span class="text-xs text-faint" data-testid="approval-queue">{t('moreWaiting', { n: queue.length })}</span>{/if}
        {#if waiting && left !== null}
          <span class={cn('ms-auto inline-flex items-center gap-1.5 font-mono text-xs tabular-nums', left <= 60_000 ? 'text-warn' : 'text-faint')}>
            <Icon name="time" class="size-3.5" />{expired ? stateLabel('expired') : t('expiresIn', { time: clock(left) })}
          </span>
        {/if}
      </div>
      <h1 class="mt-3 text-[22px] leading-tight font-[650] tracking-[-0.025em] text-balance" data-testid="approval-title">{title}</h1>
      <div class="mt-2.5 flex min-w-0 items-center gap-2.5">
        <span class="grid size-9 shrink-0 place-items-center rounded-[10px] bg-pale text-sm font-semibold text-forest">{(who.name || '?').slice(0, 1).toUpperCase()}</span>
        <div class="min-w-0">
          <b class="block truncate text-sm font-semibold">{who.name}</b>
          <span class="block truncate font-mono text-xs text-faint" data-testid={request.type === 'connect' ? 'approval-origin' : undefined}>{who.detail}</span>
        </div>
      </div>

      <ol class="mt-4 grid auto-cols-fr grid-flow-col rounded-xl border border-line bg-paper px-1.5 pt-3 pb-2.5" aria-label={t('from')}>
        {#each trail as hop, i (i)}
          <li class="relative grid min-w-0 justify-items-center gap-0.5 px-0.5 text-center">
            {#if i > 0}
              <span
                class={cn('absolute top-3.5 end-[calc(50%+17px)] w-[calc(100%-34px)] border-t-[1.5px] border-edge', hop.relayed && 'border-dashed')}
                aria-hidden="true"
              ></span>
            {/if}
            <span
              class={cn(
                'mb-1 grid size-7 place-items-center rounded-full border',
                i === trail.length - 1 ? 'border-forest bg-forest text-on-forest' : 'border-edge-soft bg-surface text-ink-2'
              )}><Icon name={hop.icon} class="size-3.5" /></span
            >
            <!-- Host names break only after a dot. -->
            <b class="max-w-full text-[11.5px] leading-tight font-semibold break-words"
              >{#if hop.name}{#each hop.name.split(/(?<=\.)/) as part, j (j)}{part}<wbr />{/each}{:else}{t(hop.role)}{/if}</b
            >
            {#if hop.name}<small class="text-[10.5px] leading-tight text-faint">{t(hop.role)}</small>{/if}
          </li>
        {/each}
      </ol>
      {#if request.type === 'connect'}
        <p class="mt-2 flex gap-1.5 text-xs text-muted" data-testid="approval-relayed"><Icon name="info" class="mt-0.5 size-3.5 text-faint" />{t('relayed')}</p>
      {/if}

      {#if waiting && request.error}
        <div class="mt-4 flex items-start gap-2 rounded-xl bg-warn-pale p-3 text-[13px]" data-testid="approval-last-error">
          <Icon name="warning" class="mt-0.5 size-[15px] shrink-0 text-warn" />
          <div class="min-w-0 flex-1">
            <p>{t('notRunYet')} {reason(request.error)}</p>
            {#if request.error === 'AUTH_REQUIRED'}
              <button class={cn(buttonClass('secondary', 'sm'), 'mt-2')} onclick={() => showMain('/login')}>{t('loginInMain')}</button>
            {/if}
          </div>
        </div>
      {/if}

      {#if waiting && (blockers.length || readiness?.warnings?.length)}
        <ul class="mt-4 grid gap-2 rounded-xl border border-warn/35 bg-warn-pale p-3 text-[13px]" data-testid="approval-blockers">
          {#each [...blockers, ...(readiness?.warnings ?? [])] as blocker (blocker)}
            <li class="flex flex-wrap items-center gap-2">
              <Icon name="warning" class="size-[15px] text-warn" />
              <span class="min-w-0 flex-1">{reason(blocker)}</span>
              {#if fixes[blocker]}<button class={buttonClass('secondary', 'xs')} onclick={fixes[blocker].run}>{t(fixes[blocker].label)}</button>{/if}
            </li>
          {/each}
        </ul>
        {#if walletReadiness?.blockers?.length}
          <button class={cn(buttonClass('secondary', 'sm'), 'mt-2')} onclick={loadReadiness} data-testid="approval-wallet-retry">{w('retry')}</button>
        {/if}
      {/if}

      <section class="mt-5">
        <h2 class="mb-2 text-xs font-semibold tracking-[0.03em] text-faint">{t(section)}</h2>
        {#if request.detail === null}
          <p class="rounded-xl border border-line bg-paper px-4 py-3.5 text-[13px] text-muted" data-testid="approval-detail-unavailable">{t('approvalDetailUnavailable')}</p>
        {:else if s}
          <div class="rounded-xl border border-line bg-paper px-4 py-3.5">
            <div class="font-mono text-[11px] text-faint">{s.profile}{s.subject ? ` · ${s.subject}` : ''}</div>
            {#if s.content?.text}<p class="mt-1.5 text-base leading-normal whitespace-pre-wrap [overflow-wrap:anywhere]" data-testid="approval-text">{s.content.text}</p>{/if}
            {#if s.content?.sha256}
              <p class="mt-1.5 font-mono text-xs break-all text-ink-2">sha256 {s.content.sha256}</p>
              {#if d.file}
                <p class="mt-1.5 flex flex-wrap items-center gap-x-1.5 text-xs text-ink-2" data-testid="approval-file">
                  <Icon name={d.file.library ? 'folder' : 'file'} class="size-3.5 text-faint" /><b class="font-medium">{d.file.name}</b>
                  <span class="text-faint">{size(d.file.size)} · {d.file.library ? t('fromLibrary') : d.file.path}</span>
                </p>
                <p class={cn('mt-1 text-xs', d.file_checked ? 'text-ok' : 'text-warn')}>{d.file_checked ? t('fileChecked') : t('fileChanged')}</p>
              {:else if request.file_check}
                <p class={cn('mt-1.5 text-xs', request.file_check.matches ? 'text-ok' : 'text-danger')} data-testid="approval-file-check">
                  {request.file_check.matches ? t('fileMatches', { name: request.file_check.name }) : t('fileDiffers', { name: request.file_check.name })}
                </p>
              {:else}
                <p class="mt-1.5 text-xs text-warn">{t('digestOnly')}</p>
              {/if}
              {#if waiting && !d.file}
                <button class={cn(buttonClass('secondary', 'sm'), 'mt-2')} disabled={busy} onclick={checkFile} data-testid="approval-check-file">
                  <Icon name="file" />{t('compareFile')}
                </button>
              {/if}
            {/if}
            {#if s.profile === 'app_action'}
              {#if rendered}
                <p class="mt-1.5 text-base font-semibold" data-testid="approval-command">{pickLabel(rendered.title, lang)}</p>
                <dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]" data-testid="approval-fields">
                  {@render row(t('application'), s.content.app_id, { testid: 'approval-app' })}
                  {#each rendered.fields as f (f.name)}{@render field(f)}{/each}
                  {#each rendered.files as f, i (i)}{@render row(t('file'), `${f.name} · r${f.revision} · ${size(f.size)} · sha256 ${f.sha256}`, { mono: true })}{/each}
                </dl>
                <p class="mt-2 text-xs text-warn">{t('appActionWarn')}</p>
              {:else}
                <dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
                  {@render row(t('application'), s.content.app_id, { testid: 'approval-app' })}
                  {@render row(t('command'), s.content.command, { testid: 'approval-command' })}
                  {#each s.content.args as arg (arg.name)}{@render row(arg.name, arg.value, { mono: true })}{/each}
                </dl>
                <p class="mt-2 text-xs text-warn" data-testid="approval-unrendered">{t('actionUnrendered')}</p>
              {/if}
            {/if}
          </div>
          <dl class="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
            {@render row(t('issuer'), s.issuer, { mono: true })}
            {#if request.type === 'connect'}{@render row(t('account'), d.account, { mono: true })}{/if}
            {#if d.origin && request.type === 'attest'}{@render row(t('origin'), d.origin)}{/if}
            {#if d.purpose ?? readiness?.purpose}{@render row(t('purpose'), purpose(d.purpose ?? readiness?.purpose))}{/if}
            {#if s.content?.content_type}{@render row(t('contentType'), s.content.content_type, { mono: true })}{/if}
            {#if s.content?.location}{@render row(t('location'), s.content.location, { mono: true })}{/if}
            {#if s.issued_at}{@render row(t('issuedAt'), when(s.issued_at * 1000))}{/if}
            {#if readiness?.usage}{@render row(t('usage'), t('usageLeft', { n: readiness.usage.remaining, time: when(readiness.usage.read_at) }))}{/if}
          </dl>
        {:else if terms || request.type === 'letter' || (request.type === 'handle' && d.total) || unquoted}
          <div class="rounded-xl border border-line bg-paper px-4 py-3.5">
            {#if terms?.terms?.method === 'Cash'}
              <div class="text-[28px] leading-tight font-[650] tracking-[-0.03em] tabular-nums" data-testid="approval-amount">
                {units(terms.terms.amount, terms.terms.decimals)} {terms.terms.asset}
              </div>
              <div class="mt-0.5 text-xs text-faint tabular-nums">+ {t('fee')} {units(terms.terms.fee_reserve, terms.terms.decimals)} {terms.terms.asset}</div>
            {:else if terms?.terms}
              <div class="text-[28px] leading-tight font-[650] tracking-[-0.03em] tabular-nums" data-testid="approval-amount">
                {units(terms.terms.required_stake_e8s, 8)} PANDA
              </div>
              <div class="mt-0.5 text-xs text-faint">{t('stake')}</div>
            {:else if request.type === 'letter'}
              <div class="text-[28px] leading-tight font-[650] tracking-[-0.03em] tabular-nums" data-testid="approval-amount">{d.amount}</div>
              <div class="mt-0.5 text-xs text-faint">{t('baseUnits')}</div>
            {:else if request.type === 'handle'}
              <div class="text-[28px] leading-tight font-[650] tracking-[-0.03em] tabular-nums" data-testid="approval-amount">{units(d.total, 8)} PANDA</div>
            {:else}
              <div class="text-[28px] leading-tight font-[650] tracking-[-0.03em] tabular-nums">${units(d.content?.amount_usd_micros ?? '0', 6)}</div>
              <div class="mt-0.5 text-xs text-faint">{method(d.content?.method)}</div>
            {/if}
          </div>
          <dl class="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
            {#if request.type === 'checkout'}
              {@render row(t('plan'), `${bought(terms)} · ${terms.origin}`, { testid: 'approval-sku' })}
              {@render row(t('price'), `$${units(terms.amount_usd_micros, 6)}`)}
              {@render row(t('serviceTerm'), `${when(terms.starts_at_ms)} – ${when(terms.expires_at_ms)}`, { testid: 'approval-period' })}
              {#if checkoutNotes(terms)}{@render row(t('note'), checkoutNotes(terms), { testid: 'approval-note' })}{/if}
            {:else if request.type === 'connect'}
              {@render row(t('plan'), `${d.content?.product_id} · ${d.content?.sku}`)}
              {@render row(t('price'), `$${units(d.content?.amount_usd_micros ?? '0', 6)} · ${method(d.content?.method)}`)}
            {/if}
            {#if request.type === 'letter'}
              {@render row(t('recipient'), d.recipient, { mono: true, testid: 'approval-recipient' })}
              {@render row(t('ledger'), d.ledger, { mono: true })}
            {/if}
            {#if request.type === 'handle'}
              {@render nameRows()}
            {/if}
            {#if terms?.terms?.method === 'Cash'}
              {@render row(t('ledger'), terms.terms.ledger, { mono: true })}
              {@render row(t('deadline'), when(terms.terms.funding_deadline_ms))}
            {:else if terms?.terms}
              {@render row(t('neuron'), terms.terms.neuron, { mono: true })}
              {@render row(t('deadline'), when(terms.terms.application_deadline_ms))}
            {/if}
            {#if terms}{@render row(t('payer'), terms.actor, { mono: true })}{/if}
          </dl>
        {:else}
          <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 rounded-xl border border-line bg-paper px-4 py-3.5 text-[13px]">
            {#if authenticating}
              {@render row(t('authPurpose'), authPurposes[d.content?.purpose] ? t(authPurposes[d.content?.purpose]) : String(d.content?.purpose ?? ''), { testid: 'approval-purpose' })}
              {@render row(t('account'), d.account, { mono: true })}
              {@render row(t('authReceiver'), String(d.content?.receiver ?? ''), { mono: true, testid: 'approval-receiver' })}
              {@render row(t('expires'), when(d.content?.expires_at))}
            {:else if request.type === 'account'}
              {@render row(t('change'), change(d.change), { testid: 'approval-change' })}
              {#if d.principal}{@render row(t('loginPrincipal'), d.principal, { mono: true, testid: 'approval-principal' })}{/if}
              {#if d.change === 'policy'}
                {@const was = d.previous}
                {@render row(t('signingPolicy'), `${was ? `${was.frozen ? t('frozen') : t('policyOpen')} → ` : ''}${d.frozen ? t('frozen') : t('policyOpen')}`, { testid: 'approval-policy' })}
                {@render row(t('dailyExecutions'), `${was ? `${was.daily_executions} → ` : ''}${d.daily_executions}`)}
                {@render row(t('purposes'), `${was ? `${purposes(was.allowed_purposes)} → ` : ''}${purposes(d.allowed_purposes)}`)}
              {/if}
              {#if d.delay_ms}{@render row(t('recoveryDelay'), t('daysCount', { n: Math.round(d.delay_ms / 86_400_000) }))}{/if}
              {#if d.op_id}{@render row(t('recovery'), d.op_id, { mono: true })}{/if}
            {:else if request.type === 'delegation'}
              {#if d.kind?.type === 'grant'}
                {@render row(t('grantee'), agents[d.kind.subject] ? `${agents[d.kind.subject]} · ${d.kind.subject}` : d.kind.subject, { mono: true, testid: 'approval-subject' })}
                {@render row(t('scopeLabel'), d.kind.scopes.join(', '), { mono: true })}
                {@render row(t('audienceLabel'), d.kind.audiences.join(', '), { mono: true })}
                {@render row(t('expires'), when(d.kind.expires_at))}
                {#if d.kind.relationship}{@render row(t('relationship'), d.kind.relationship, { testid: 'approval-relationship' })}{/if}
                {@render row('ID', d.kind.id, { mono: true })}
              {:else}
                {@render row('ID', d.kind?.id ?? '', { mono: true })}
                {#if d.kind?.reason}{@render row(t('revokeReason'), d.kind.reason)}{/if}
              {/if}
            {:else if request.type === 'grant'}
              {@render row(t('grantee'), agents[d.grantee] ? `${agents[d.grantee]} · ${d.grantee}` : d.grantee, { mono: true, testid: 'approval-grantee' })}
              {@render row(t('capabilities'), (d.capabilities ?? []).join('\n'), { mono: true, multiline: true, testid: 'approval-capabilities' })}
              {@render row(t('grantLifetime'), d.days ? t('daysCount', { n: d.days }) : t('untilRevoked'))}
            {:else if request.type === 'principal'}
              {@render row(t('method'), d.change, { testid: 'approval-change' })}
              {#if d.generation}{@render row(t('controllers'), `#${d.generation}${d.name ? ` · ${d.name}` : ''}`)}{/if}
              {#if d.delegation}{@render row(t('ceiling'), d.delegation === 'Unrestricted' ? t('everything') : JSON.stringify(d.delegation), { mono: d.delegation !== 'Unrestricted' })}{/if}
              {#if d.principal_type}{@render row(t('principal'), d.principal_type)}{/if}
              {#if d.invalid_from}{@render row(t('deadline'), when(d.invalid_from))}{/if}
            {:else if request.type === 'device_capabilities'}
              {@render row(t('device'), `${d.device_id}${d.this_device ? ` (${t('thisDevice')})` : ''}`, { mono: true })}
              {@render row(t('capabilities'), d.capabilities?.join(', ') ?? '')}
            {:else if request.type === 'device_revoke'}
              {@render row(t('device'), `${d.device_id}${d.this_device ? ` (${t('thisDevice')})` : ''}`, { mono: true })}
            {:else if request.type === 'handle'}
              {@render nameRows()}
            {:else if request.type === 'wallet'}
              {@render row(t('twWallet'), `${d.name}${d.wallet ? ` · ${d.wallet}` : ''}`, { testid: 'approval-wallet' })}
              {#if d.action === 'create'}
                {@render row(t('price'), `${units(d.price_e8s, 8)} ICP + ${units(d.ledger_fee_e8s, 8)}`, { testid: 'approval-amount' })}
                {@render row(t('twPolicy'), t('twPolicyLine', { approvals: d.policy.approver_threshold, admin: d.policy.admin_threshold, version: d.policy.version }))}
                <div class="col-span-2"><WalletIntent intent={{kind:'set_policy',policy:d.policy}} /></div>
                {#each d.members as m (m.id)}{@render row(w(m.role), `${m.id}${m.label ? ` · ${m.label}` : ''}${m.agent_ids?.length ? ` · ${m.agent_ids.join(', ')}` : ''}`, { mono: true })}{/each}
              {:else if d.proposal}
                <div class="col-span-2" data-testid="approval-proposal">
                  <WalletIntent intent={d.proposal.intent} metadata={walletAssets} before={d.before} />
                </div>
                {@render row(t('twProposal'), `#${d.proposal.id}`)}
                {@render row(t('twMember'), d.proposal.proposer, { mono: true })}
                {#if d.proposal.via_agent}{@render row('Agent ID', d.proposal.via_agent, { mono: true })}{/if}
                {@render row(t('twApprovals'), `${d.proposal.requirement.approvals} · ${w(d.proposal.requirement.admin ? 'Admin' : 'Approver')}`)}
                {@render row(w('delay_ms'), String(d.proposal.requirement.delay_ms))}
                {@render row(t('expires'), when(d.proposal.expires_at))}
                {#if d.plan}
                  {@render row(w('plan'), `#${d.plan.seq} · ${d.plan.chain} · ${d.plan.from}`, { mono: true })}
                  {@render row(w('fee'), `${d.plan.fee} ${w('atomic')}`, { mono: true })}
                  {#each d.plan.outputs ?? [] as o,i (i)}{@render row(w('recipient'), `${o.amount} ${w('atomic')} → ${o.to}`, { mono: true })}{/each}
                  {#if d.plan.change}{@render row(t('twChange'), `${d.plan.change.amount} → ${d.plan.change.to}`, { mono: true })}{/if}
                  {@render row(t('expires'), when(d.plan.expires_at))}
                {/if}
                {#if d.collect_only}{@render row(w('cosigners'), w('collectHint'))}{/if}
                {@render row(t('digest'), d.proposal.digest, { mono: true })}
              {/if}
            {/if}
          </dl>
        {/if}
      </section>

      {#if after.length}
        <section class="mt-5">
          <h2 class="mb-2 text-xs font-semibold tracking-[0.03em] text-faint">{t('afterApproval')}</h2>
          <ul class="grid gap-1.5">
            {#each after as [line, warn], i (i)}
              <li class={cn('grid grid-cols-[16px_minmax(0,1fr)] gap-2 text-[13px]', warn ? 'text-warn' : 'text-ink-2')}>
                <Icon name={warn ? 'warning' : 'info'} class={cn('mt-0.5 size-[15px]', !warn && 'text-faint')} /><span>{line}</span>
              </li>
            {/each}
            {#if request.risk === 'high'}
              <li class="grid grid-cols-[16px_minmax(0,1fr)] gap-2 text-[13px] text-ink-2"><Icon name="security" class="mt-0.5 size-[15px] text-faint" /><span>{t('afterPresence')}</span></li>
            {/if}
          </ul>
        </section>
      {/if}

      <Collapsible class="mt-4 rounded-lg border border-line px-2.5 py-1">
        {#snippet title()}<span class="text-[12.5px] text-muted">{t('technical')}</span>{/snippet}
        <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 pb-2 text-xs" data-testid="approval-technical">
          {@render row(t('requestId'), request.id, { mono: true })}
          {@render row(t('digest'), request.digest, { mono: true })}
          {@render row(t('from'), request.source, { mono: true, testid: 'approval-source' })}
          {#if request.type === 'connect' && request.detail !== null}
            {@render row(t('method'), `${d.method} · v${d.app_version}`, { mono: true })}
            {@render row(t('operation'), d.operation_id ?? '', { mono: true })}
            {@render row(t('requested'), when(d.requested_at))}
          {/if}
          {#if s?.profile === 'app_action'}
            {@render row(t('operation'), s.content.operation_id, { mono: true })}
            {@render row(t('expires'), when(s.content.expires_at))}
          {/if}
          {#if request.type === 'delegation' && request.detail !== null}{@render row(t('controllers'), `#${d.generation} · ${d.kind?.id ?? ''}`, { mono: true })}{/if}
          {@render row(t('status'), stateLabel(request.state))}
          {@render row(t('created'), when(request.created_at))}
          {#if request.decided_at}{@render row(t('decided'), `${when(request.decided_at)} · ${request.decided_via}`, { testid: 'approval-decided' })}{/if}
          {#if request.execution}{@render row(t('execution'), request.execution, { mono: true })}{/if}
          {#if request.error}{@render row(t('error'), request.error, { mono: true })}{/if}
          {@render row(t('expires'), when(request.expires_at))}
        </dl>
      </Collapsible>

      {#if unquoted && waiting}
        <div class="mt-4 rounded-xl bg-warn-pale p-3" data-testid="checkout-quote">
          <p class="text-[13px] text-warn">{t('quoteFirst')}</p>
          <div class="mt-2 flex items-center gap-2">
            {#if d.content?.method === 'Cash'}
              <Select bind:value={selection} testid="quote-asset" placeholder={t('choose')} options={assets.map((a) => ({ value: a.ledger, label: a.asset }))} />
            {:else}
              <input placeholder={t('neuronHint')} bind:value={selection} />
            {/if}
            <button class={buttonClass()} disabled={busy || !selection.trim()} onclick={quote}>{t('quote')}</button>
          </div>
        </div>
      {/if}
      {#if error}<p class="mt-3 text-[13px] text-danger" data-testid="approval-error">{error}</p>{/if}
    </div>

    <!-- The ticket's perforation: above it what you review, below it the decision. -->
    <div class="relative mx-4 h-0 shrink-0 border-t-[1.5px] border-dashed border-line" aria-hidden="true">
      <span class="absolute -top-2 -start-6 size-4 rounded-full border border-line bg-paper"></span>
      <span class="absolute -top-2 -end-6 size-4 rounded-full border border-line bg-paper"></span>
    </div>
    <div class="shrink-0 px-5 pt-3.5 pb-4">
      {#if waiting && !approving}
        {#if left !== null}
          <div class="h-[3px] overflow-hidden rounded-full bg-line" aria-hidden="true">
            <i
              class={cn('block h-full transition-[width] duration-1000 ease-linear', left <= 60_000 ? 'bg-warn' : 'bg-forest')}
              style:width="{Math.min(100, (left / lifetime) * 100)}%"
            ></i>
          </div>
        {/if}
        <div class="mt-3 grid grid-cols-[1fr_1.4fr] gap-2.5">
          <button bind:this={rejectButton} class={buttonClass('secondary', 'lg')} disabled={busy || expired} data-testid="reject" onclick={() => decide(false)}>
            {t('reject')}
          </button>
          <button
            class={buttonClass('primary', 'lg', 'overflow-hidden disabled:border-transparent disabled:bg-forest/45 disabled:opacity-100')}
            disabled={!canApprove}
            data-testid="approve"
            onclick={() => !hold && decide(true)}
            onpointerdown={(event) => hold && (event.preventDefault(), startHold())}
            onpointerup={stopHold}
            onpointerleave={stopHold}
            onpointercancel={stopHold}
            onkeydown={(event) => hold && holdKey(event, true)}
            onkeyup={(event) => hold && holdKey(event, false)}
          >
            {#if waiting && !ready && !expired}
              {#key request.digest}<span class="absolute inset-0 origin-left animate-[approve-delay_1.2s_linear_forwards] bg-on-forest/20 rtl:origin-right"></span>{/key}
            {/if}
            {#if hold}<span class="absolute inset-0 origin-left bg-on-forest/25 rtl:origin-right" style:transform="scaleX({held})"></span>{/if}
            <span class="relative">{hold ? t('holdToApprove') : t('approve')}</span>
          </button>
        </div>
        <p class="mt-2 text-center text-[11.5px] text-faint">
          {blockers.length ? t('cannotSignYet') : unquoted ? t('quoteFirst') : !ready ? t('approveSoon') : hold ? t('holdHint') : t('approveHint')}
        </p>
      {:else}
        <ol class="grid gap-2" data-testid="approval-outcome">
          {#each steps as step, i (i)}
            <li class="flex items-center gap-2.5 text-[13.5px]">
              <span
                class={cn(
                  'grid size-5 place-items-center rounded-full',
                  step.tone === 'ok' && 'bg-ok text-surface',
                  step.tone === 'danger' && 'bg-danger text-surface',
                  step.tone === 'warn' && 'bg-warn text-surface',
                  step.tone === 'muted' && 'bg-muted text-surface',
                  step.tone === 'pending' && 'border-[1.5px] border-edge-soft text-faint'
                )}><Icon name={step.icon} class={cn('size-3', step.tone === 'pending' && 'animate-spin motion-reduce:animate-none')} /></span
              >
              {step.label ? t(step.label) : stateLabel(step.state ?? '')}
            </li>
          {/each}
        </ol>
        {#if ['failed', 'lost', 'unknown'].includes(request.state) && request.error}
          <p class="mt-2 text-[12.5px] text-muted" data-testid="approval-reason">{reason(request.error)}</p>
          {#if request.error === 'PENDING' && (request.type === 'checkout' || request.detail?.method === 'checkout')}
            <button class={cn(buttonClass('secondary', 'sm'), 'mt-2')} onclick={() => showMain('/plan?tab=payments')} data-testid="approval-open-orders">
              {t('openOrders')}
            </button>
          {/if}
        {/if}
        {#if !approving && queue.length}
          <button class={cn(buttonClass('secondary'), 'mt-3 w-full')} onclick={() => (location.hash = `/approval/${queue[0]}`)} data-testid="approval-next">
            {t('nextRequest', { n: queue.length })}<Icon name="chevron-end" />
          </button>
        {/if}
      {/if}
    </div>
  {/if}
</div>
