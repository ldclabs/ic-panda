<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { t, stateLabel, type Label } from '../i18n/index.svelte'
  import { call, code, units, when } from '../rpc'
  import { holdToApprove, kindOf, trailOf } from '../approval'
  import Collapsible from '../components/Collapsible.svelte'
  import type { Glyph } from '../components/glyphs'
  import Icon from '../components/Icon.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass, cn } from '../components/ui'

  /**
   * The approval window, laid out as a ticket: who asks and the way the
   * request came, what exactly is signed, paid or changed, what approving
   * does, and the technical fields folded below. Only this window decides,
   * with the digest of what it shows. Approve opens after a short delay and
   * again whenever the request changes; Reject has the focus; payments,
   * device revocation and control of the account take a one-second hold.
   */
  let { id }: { id: string } = $props()
  let request = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  let assets = $state<any[]>([])
  let selection = $state('')
  let now = $state(Date.now())
  let ready = $state(false)
  let held = $state(0)
  let rejectButton = $state<HTMLButtonElement>()
  let delay: ReturnType<typeof setTimeout> | undefined
  let frame = 0

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
      request = next
      if (first && next?.state === 'awaiting_user') {
        await tick()
        rejectButton?.focus()
      }
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    }
  }

  /** Decide exactly what is shown: the digest of this view goes with the answer. */
  async function decide(approve: boolean) {
    busy = true
    error = ''
    try {
      request = await window.dmsg.decide(request.id, request.digest, approve)
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
      await load()
    } finally {
      busy = false
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
      error = `${t('error')}: ${code(e)}`
    } finally {
      busy = false
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

  onMount(() => {
    void load().then(async () => {
      if (request?.detail?.method === 'checkout' && request.detail.content?.method === 'Cash') {
        assets = await call('plan/assets').catch(() => [])
        if (assets.length) selection = assets[0].ledger
      }
    })
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'approval/changed') void load()
    })
    const clock = setInterval(() => (now = Date.now()), 1000)
    return () => {
      off()
      clearInterval(clock)
      clearTimeout(delay)
      stopHold()
    }
  })

  const d = $derived(request?.detail ?? {})
  /** A statement to sign: this device's own, or a page's document or action. */
  const s = $derived(d.statement ?? d.content?.statement ?? (d.method === 'signAction' ? d.content : null))
  /** Terms of a checkout: a web checkout's quote, or this device's own. */
  const terms = $derived(request?.type === 'connect' ? d.checkout : request?.type === 'checkout' ? d : null)
  const unquoted = $derived(request?.type === 'connect' && d.method === 'checkout' && !d.checkout)
  const waiting = $derived(request?.state === 'awaiting_user')
  const left = $derived(request?.expires_at ? Math.max(0, request.expires_at - now) : null)
  const expired = $derived(left === 0)
  /** How long the request had from its creation, for the bar of the time left. */
  const lifetime = $derived(Math.max(1, (request?.expires_at ?? 0) - (request?.created_at ?? (request?.expires_at ?? 0) - 600_000)))
  const hold = $derived(holdToApprove(request))
  const canApprove = $derived(waiting && ready && !busy && !unquoted && !expired)
  const trail = $derived(trailOf(request))
  const kind = $derived(kindOf(request))

  /** An account security change for people; unknown changes show as they are. */
  const change = (name: string): string =>
    ({
      bind_login: t('bindLogin'),
      remove_login: `${t('remove')} ${t('loginPrincipal')}`,
      policy: t('signingPolicy'),
      recovery_delay: t('recoveryDelay'),
      dispute_recovery: t('disputeRecovery')
    })[name] ?? name

  const title = $derived.by(() => {
    if (!request) return ''
    switch (request.type) {
      case 'connect':
      case 'attest':
        if (d.method === 'checkout') return t('payPlan')
        if (d.method === 'authenticate') return t('authenticateTitle')
        if (s?.profile === 'app_action') return t('appAction')
        return s?.content?.sha256 ? t('signFile') : t('newStatement')
      case 'checkout':
        return t('payPlan')
      case 'letter':
        return t('payLetter')
      case 'handle':
        return `${t('names')}: ${d.name}`
      case 'delegation':
        return d.kind?.type === 'revoke' ? t('delegationRevoke') : t('delegationGrant')
      case 'principal':
        return t('principalChange')
      case 'account':
        return change(d.change)
      case 'device_capabilities':
        return t('capabilities')
      case 'device_revoke':
        return `${t('revoke')} · ${t('device')}`
      default:
        return request.type
    }
  })

  /** Who asks: the web application, the agent, the terminal or this device's main window. */
  const who = $derived(
    request?.type === 'connect' ? { name: String(d.app_id ?? ''), detail: String(d.origin ?? '') } : { name: trail[0]?.name ?? '', detail: t(trail[0]?.role ?? 'sourceOther') }
  )

  /** A web page asks this device to vouch that the account approved the page's session. */
  const authenticating = $derived(request?.type === 'connect' && d.method === 'authenticate')

  const section = $derived<Label>(s || authenticating ? 'youSign' : kind === 'kindPayment' ? 'youPay' : 'youChange')

  /** What approving does, in plain words; the last lines say what cannot be undone. */
  const after = $derived.by((): [string, boolean][] => {
    if (!request) return []
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
        return d.total ? [pay, final] : [[t('afterName'), false]]
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
      default:
        return []
    }
  })

  /** The decision's progress, from the daemon's states; each step is a real event. */
  const steps = $derived.by((): { label: string; icon: Glyph; tone: string }[] => {
    const state = request?.state
    if (!state || state === 'awaiting_user') return []
    if (['rejected', 'expired', 'cancelled', 'elsewhere'].includes(state)) return [{ label: stateLabel(state), icon: 'close', tone: 'muted' }]
    const list = [{ label: t('stepApproved'), icon: 'check' as Glyph, tone: 'ok' }]
    if (state === 'executing') list.push({ label: stateLabel(state), icon: 'pending', tone: 'pending' })
    else list.push({ label: stateLabel(state), icon: state === 'done' ? 'check' : 'warning', tone: state === 'done' ? 'ok' : state === 'failed' ? 'danger' : 'warn' })
    return list
  })

  const clock = (ms: number) => {
    const seconds = Math.ceil(ms / 1000)
    return seconds >= 6000 ? when(request?.expires_at) : `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
  }
</script>

{#snippet row(label: string, value: string, options: { mono?: boolean; testid?: string } = {})}
  <dt class="text-faint">{label}</dt>
  <dd class={cn('min-w-0 [overflow-wrap:anywhere] text-ink-2', options.mono && 'font-mono text-xs leading-5')} data-testid={options.testid}>{value}</dd>
{/snippet}

<div class="flex h-screen flex-col overflow-hidden bg-surface text-[13.5px] text-ink">
  {#if inset}
    <div class="h-9 shrink-0 border-b border-line bg-paper-dim text-center text-xs leading-9 text-faint [-webkit-app-region:drag]">{t('approvalWindow')}</div>
  {/if}
  {#if !request}
    <p class="p-5 text-muted">{error || '…'}</p>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto px-5 pt-4 pb-5">
      <div class="flex items-center gap-2">
        <span class="inline-flex h-6 items-center gap-1.5 rounded-md bg-gem-pale px-2 text-xs font-semibold text-gem">
          <i class="size-[7px] rotate-45 rounded-[1px] bg-current"></i>{t(kind)}
        </span>
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

      <section class="mt-5">
        <h2 class="mb-2 text-xs font-semibold tracking-[0.03em] text-faint">{t(section)}</h2>
        {#if s}
          <div class="rounded-xl border border-line bg-paper px-4 py-3.5">
            <div class="font-mono text-[11px] text-faint">{s.profile}{s.subject ? ` · ${s.subject}` : ''}</div>
            {#if s.content?.text}<p class="mt-1.5 text-base leading-normal [overflow-wrap:anywhere]" data-testid="approval-text">{s.content.text}</p>{/if}
            {#if s.content?.sha256}
              <p class="mt-1.5 font-mono text-xs break-all text-ink-2">sha256 {s.content.sha256}</p>
              <p class={cn('mt-1.5 text-xs', d.file_checked ? 'text-ok' : 'text-warn')}>
                {d.file ? (d.file_checked ? t('fileChecked') : t('fileChanged')) : t('digestOnly')}
              </p>
            {/if}
            {#if s.profile === 'app_action'}
              <dl class="mt-2 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
                {@render row(t('application'), s.content.app_id, { testid: 'approval-app' })}
                {@render row(t('command'), s.content.command, { testid: 'approval-command' })}
                {#each s.content.args as arg (arg.name)}{@render row(arg.name, arg.value, { mono: true })}{/each}
              </dl>
              <p class="mt-2 text-xs text-warn">{t('appActionWarn')}</p>
            {/if}
          </div>
          <dl class="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
            {@render row(t('issuer'), s.issuer, { mono: true })}
            {#if request.type === 'connect'}{@render row(t('account'), d.account, { mono: true })}{/if}
            {#if d.origin && request.type === 'attest'}{@render row(t('origin'), d.origin)}{/if}
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
              <div class="mt-0.5 text-xs text-faint">{d.content?.method}</div>
            {/if}
          </div>
          <dl class="mt-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[13px]">
            {#if request.type === 'checkout'}
              {@render row(t('plan'), `${terms.sku} · ${terms.origin}`, { testid: 'approval-sku' })}
              {@render row(t('price'), `$${units(terms.amount_usd_micros, 6)}`)}
            {:else if request.type === 'connect'}
              {@render row(t('plan'), `${d.content?.product_id} · ${d.content?.sku}`)}
              {@render row(t('price'), `$${units(d.content?.amount_usd_micros ?? '0', 6)} · ${d.content?.method}`)}
            {/if}
            {#if request.type === 'letter'}
              {@render row(t('recipient'), d.recipient, { mono: true, testid: 'approval-recipient' })}
              {@render row(t('ledger'), d.ledger, { mono: true })}
            {/if}
            {#if request.type === 'handle'}
              {@render row(t('names'), d.name)}
              {@render row(t('method'), d.action)}
              {@render row(t('payer'), d.caller, { mono: true })}
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
              {@render row(t('authPurpose'), String(d.content?.purpose ?? ''), { testid: 'approval-purpose' })}
              {@render row(t('account'), d.account, { mono: true })}
              {@render row(t('authReceiver'), String(d.content?.receiver ?? ''), { mono: true, testid: 'approval-receiver' })}
              {@render row(t('expires'), when(d.content?.expires_at))}
            {:else if request.type === 'account'}
              {@render row(t('change'), change(d.change), { testid: 'approval-change' })}
              {#if d.principal}{@render row(t('loginPrincipal'), d.principal, { mono: true, testid: 'approval-principal' })}{/if}
              {#if d.change === 'policy'}
                {@render row(t('signingPolicy'), `${d.frozen ? t('frozen') : t('formalOn')} · ${d.daily_executions} · ${d.allowed_purposes?.join(', ')}`)}
              {/if}
              {#if d.delay_ms}{@render row(t('recoveryDelay'), String(Math.round(d.delay_ms / 86_400_000)))}{/if}
              {#if d.op_id}{@render row(t('recovery'), d.op_id, { mono: true })}{/if}
            {:else if request.type === 'delegation'}
              {#if d.kind?.type === 'grant'}
                {@render row(t('grantee'), d.kind.subject, { mono: true, testid: 'approval-subject' })}
                {@render row(t('scopeLabel'), d.kind.scopes.join(', '), { mono: true })}
                {@render row(t('audienceLabel'), d.kind.audiences.join(', '), { mono: true })}
                {@render row(t('expires'), when(d.kind.expires_at))}
                {#if d.kind.relationship}{@render row(t('subject'), d.kind.relationship)}{/if}
              {:else}
                {@render row('ID', d.kind?.id ?? '', { mono: true })}
              {/if}
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
              {@render row(t('device'), d.device_id, { mono: true })}
            {:else if request.type === 'handle'}
              {@render row(t('names'), d.name)}
              {@render row(t('method'), d.action)}
              {@render row(t('payer'), d.caller, { mono: true })}
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
          </ul>
        </section>
      {/if}

      <Collapsible class="mt-4 rounded-lg border border-line px-2.5 py-1">
        {#snippet title()}<span class="text-[12.5px] text-muted">{t('technical')}</span>{/snippet}
        <dl class="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 pb-2 text-xs">
          {@render row(t('requestId'), request.id, { mono: true })}
          {@render row(t('digest'), request.digest, { mono: true })}
          {@render row(t('from'), request.source, { mono: true, testid: 'approval-source' })}
          {#if request.type === 'connect'}
            {@render row(t('method'), `${d.method} · v${d.app_version}`, { mono: true })}
            {@render row(t('operation'), d.operation_id ?? '', { mono: true })}
            {@render row(t('requested'), when(d.requested_at))}
          {/if}
          {#if s?.profile === 'app_action'}
            {@render row(t('operation'), s.content.operation_id, { mono: true })}
            {@render row(t('expires'), when(s.content.expires_at))}
          {/if}
          {#if request.type === 'delegation'}{@render row(t('controllers'), `#${d.generation} · ${d.kind?.id ?? ''}`, { mono: true })}{/if}
          {@render row(t('status'), stateLabel(request.state))}
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
      {#if waiting}
        {#if left !== null}
          <div class="h-[3px] overflow-hidden rounded-full bg-line" aria-hidden="true">
            <i
              class={cn('block h-full transition-[width] duration-1000 ease-linear', left <= 60_000 ? 'bg-warn' : 'bg-forest')}
              style:width="{Math.min(100, (left / lifetime) * 100)}%"
            ></i>
          </div>
        {/if}
        <div class="mt-3 grid grid-cols-[1fr_1.4fr] gap-2.5">
          <button bind:this={rejectButton} class={buttonClass('secondary', 'lg')} disabled={busy} data-testid="reject" onclick={() => decide(false)}>
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
          {unquoted ? t('quoteFirst') : !ready ? t('approveSoon') : hold ? t('holdHint') : t('approveHint')}
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
              {step.label}
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  {/if}
</div>
