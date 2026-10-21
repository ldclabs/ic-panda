<script lang="ts">
  import { onMount } from 'svelte'
  import Avatar from '../components/Avatar.svelte'
  import { copyText } from '../components/clipboard'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Page from '../components/page/Page.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import Select from '../components/Select.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, errorText, onChange, planAction, single, units, when } from '../rpc'
  import AgentAccess from './agents/AgentAccess.svelte'
  import {
    activationTotal,
    MAX_DRAFT_MEMBERS,
    memberInputs,
    memberProblem,
    thresholdLimits,
    type MemberDraft
  } from './wallet/new-wallet'

  /**
   * Team wallets: one canister per team. Proposing, executing what members
   * approved, broadcasting, reading and refunding an activation payment
   * that created no wallet run here; creating, joining, deciding and
   * cosigning open the approval window. The wallet signs every transaction;
   * this device checks each plan with its own decoder first. Each change
   * comes back through the daemon's `wallet/changed` or `approval/changed`.
   */
  let { status }: { status: any } = $props()

  let wallets = $state<any[]>([])
  let selected = $state('')
  let wallet = $state<any>(null)
  let proposals = $state<any[]>([])
  let balances = $state<any>(null)
  let audit = $state<any>(null)
  let principal = $state('')
  let cosigners = $state<any[]>([])
  // What the account's plan allows before a quote is stored.
  let preview = $state<any>(null)
  // Activation payments this device made that created no wallet.
  let deposits = $state<any[]>([])
  let error = $state('')
  let busy = $state(false)
  // New wallet.
  let name = $state('')
  let nextDraft = 0
  const blankDraft = (): MemberDraft => ({ key: nextDraft++, id: '', role: 'approver', label: '', touched: false })
  let drafts = $state<MemberDraft[]>([blankDraft()])
  let approvals = $state(1)
  let adminApprovals = $state(1)
  // A quote is read once when the view opens; the person may read it again.
  let quoteAsked = false
  let quoteLoading = $state(false)
  // Join.
  let joining = $state('')
  // Transfer.
  let from = $state('0')
  let asset = $state('icp')
  let to = $state('')
  let amount = $state('')
  let maxFee = $state('10000')
  // Cosigner key.
  let cosignerLabel = $state('')
  // This account's Agent IDs: their names, what they may do here, and which propose for this member.
  let hosted = $state<any[]>([])
  let access = $state(false)
  let delegating = $state<Record<string, boolean> | null>(null)
  /** The wallet whose balances and audit are shown; they are read again only on request. */
  let opened = ''

  const unlocked = $derived(status?.state === 'unlocked')
  const me = $derived(wallet?.me)
  const role = $derived(me?.role ?? '')
  const decider = $derived(['Admin', 'Approver'].includes(role))
  const creator = $derived(String(status?.account ?? ''))
  const roleOptions = $derived([
    { value: 'admin', label: t('twRoleAdmin') },
    { value: 'approver', label: t('twRoleApprover') },
    { value: 'proposer', label: t('twRoleProposer') },
    { value: 'viewer', label: t('twRoleViewer') },
    { value: 'recovery', label: t('twRoleRecovery') }
  ])
  // The creator is an Admin; only Admins and Approvers count toward the thresholds.
  const limits = $derived(thresholdLimits(drafts))
  const memberCount = $derived(1 + drafts.filter((row) => row.id.trim()).length)
  const problems = $derived(drafts.map((row) => memberProblem(row, drafts, creator)))
  const invalid = $derived(problems.some((problem) => problem !== null))
  const canCreate = $derived(!!name.trim() && !invalid)
  const remaining = $derived(preview?.remaining ?? 0)
  const total = $derived(preview ? activationTotal(preview.price_e8s, preview.ledger_fee_e8s) : '0')

  // A role may stop counting toward a threshold; the threshold never asks for more than there are.
  $effect(() => {
    if (approvals > limits.approvers) approvals = limits.approvers
    if (adminApprovals > limits.admins) adminApprovals = limits.admins
  })

  // The price is a fast read, so the form is ready when the view opens; a
  // failed read leaves the Get a quote button as the way to try again.
  $effect(() => {
    if (!unlocked) {
      quoteAsked = false
      return
    }
    if (quoteAsked) return
    quoteAsked = true
    void loadQuote(true)
  })

  /** Assets an address can send, as `wallet/send` reads them. */
  const assetsOf = (address: any): { value: string; label: string }[] => {
    if (!address || !wallet) return []
    const spendable: any[] = wallet.policy.spendable_assets
    const list: { value: string; label: string }[] = []
    for (const a of spendable) {
      if (address.chain === 'Icp' && a.asset === 'ICP') list.push({ value: 'icp', label: 'ICP' })
      if (address.chain === 'Icp' && a.asset === 'ICRC') list.push({ value: `icrc:${a.ledger}`, label: `ICRC ${a.ledger.slice(0, 10)}…` })
      if (address.chain === 'Bitcoin' && a.asset === 'BTC') list.push({ value: 'btc', label: 'BTC' })
      if (address.chain === 'Evm' && a.asset === 'ETH') list.push({ value: `evm:${a.chain_id}`, label: `ETH · ${a.chain_id}` })
      if (address.chain === 'Evm' && a.asset === 'ERC20') list.push({ value: `evm:${a.chain_id}:${a.token}`, label: `ERC-20 ${a.token.slice(0, 10)}… · ${a.chain_id}` })
      if (address.chain === 'Solana' && a.asset === 'SOL') list.push({ value: 'sol', label: 'SOL' })
      if (address.chain === 'Solana' && a.asset === 'SPL') list.push({ value: `sol:${a.mint}`, label: `SPL ${a.mint.slice(0, 10)}…` })
    }
    return list
  }
  const source = $derived(wallet?.addresses.find((a: any) => String(a.id) === from))
  const assets = $derived(assetsOf(source))

  $effect(() => {
    if (assets.length && !assets.some((a) => a.value === asset)) asset = assets[0].value
  })

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      toast(() => errorText(e), planAction(e))
    } finally {
      busy = false
    }
  }

  /** The hub and ledger reads behind the deposits may fail on their own; the list waits for the next load. */
  const readDeposits = () => call<any[]>('wallet/deposits').catch(() => [])

  const reload = single(async () => {
    if (!unlocked) return
    try {
      const [list, mine, keys, paid, ids] = await Promise.all([
        call('wallet/list'),
        call('wallet/principal'),
        call('wallet/cosigners'),
        readDeposits(),
        call<any>('agent/ids').catch(() => null)
      ])
      hosted = ids?.agents ?? []
      wallets = list
      principal = mine.principal
      cosigners = keys
      deposits = paid
      if (!selected && wallets.length) selected = wallets[0].wallet
      error = ''
      if (selected) await open(selected)
    } catch (e) {
      error = errorText(e)
    }
  })

  async function open(id: string) {
    selected = id
    if (opened !== id) {
      opened = id
      balances = null
      audit = null
    }
    try {
      const [shown, page] = await Promise.all([call('wallet/show', { wallet: id }), call('wallet/proposals', { wallet: id, limit: 50 })])
      wallet = shown
      proposals = page.proposals
    } catch (e) {
      wallet = null
      proposals = []
      // A wallet listing this account but not joined yet reads nothing.
      error = code(e) === 'FORBIDDEN' ? t('twNotJoined') : errorText(e)
    }
  }

  const approve = async (request: any) => bridge(() => window.dmsg.openApproval(request.id))

  const agentName = (did: string) => hosted.find((a) => a.agent_id === did)?.label ?? did

  /** Which of this account's Agent IDs may propose for its member: a membership proposal the admins approve. */
  const proposeAgents = () =>
    attempt(async () => {
      const agent_ids = Object.keys(delegating ?? {}).filter((id) => delegating?.[id])
      await call('wallet/member_agents', { wallet: selected, agent_ids })
      delegating = null
      toast(() => t('twAgentsProposed'))
    })

  /** The fast preview; quiet when the view opens it, a notice when the person asks again. */
  async function loadQuote(quiet: boolean) {
    if (quoteLoading) return
    quoteLoading = true
    if (!quiet) busy = true
    try {
      preview = await call('wallet/quote_preview')
    } catch (e) {
      if (!quiet) toast(() => errorText(e), planAction(e))
    } finally {
      quoteLoading = false
      if (!quiet) busy = false
    }
  }

  const getQuote = () => loadQuote(false)

  function addMember() {
    if (drafts.length >= MAX_DRAFT_MEMBERS) return
    drafts = [...drafts, blankDraft()]
  }

  function removeMember(key: number) {
    drafts = drafts.filter((row) => row.key !== key)
    if (!drafts.length) drafts = [blankDraft()]
  }

  function resetDraft() {
    name = ''
    drafts = [blankDraft()]
    approvals = 1
    adminApprovals = 1
  }

  /** Back to the price panel; the next quote starts a new wallet. */
  function cancelCreate() {
    resetDraft()
    preview = null
  }

  const create = () =>
    attempt(async () => {
      if (!canCreate) return
      // The preview only says what the plan allows; the hub stores the quote
      // this approval and payment commit to.
      const quote: any = await call('wallet/quote')
      if (quote.remaining === 0) {
        preview = quote
        return
      }
      await approve(
        await call('wallet/create', {
          quote: quote.quote_id,
          name: name.trim(),
          members: memberInputs(drafts),
          approvals,
          admin_approvals: adminApprovals
        })
      )
      // The form closes so another wallet starts from a fresh quote; the draft
      // stays for a retry when the approval window is dismissed.
      preview = null
    })

  /** Back to the paying account, which the daemon names; no approval, the money goes nowhere else. */
  const refund = (id: string) =>
    attempt(async () => {
      const refunded: any = await call('wallet/refund', { quote: id })
      toast(() => t('twRefunded', { to: refunded.to, block: refunded.block }))
      deposits = await readDeposits()
    })

  const join = () => attempt(async () => approve(await call('wallet/join', { wallet: joining.trim() })))

  const decide = (method: string, proposal: number) => attempt(async () => approve(await call(method, { wallet: selected, proposal })))

  const send = () =>
    attempt(async () => {
      await call('wallet/send', { wallet: selected, from: Number(from), asset, to: to.trim(), amount: amount.trim(), max_fee: maxFee.trim() })
      to = ''
      amount = ''
    })

  const execute = (proposal: number) =>
    attempt(async () => {
      const result: any = await call('wallet/execute', { wallet: selected, proposal })
      // A plan this device cosigns waits in the approval window.
      if (result.kind === 'wallet' && result.digest) await approve(result)
      else if (result.broadcast_error) toast(() => `${t('twBroadcastFailed')}: ${result.broadcast_error}`)
    })

  const step = (method: string, proposal: number) => attempt(() => call(method, { wallet: selected, proposal }))

  const freeze = () => attempt(() => call('wallet/freeze', { wallet: selected }))

  const readBalances = () => attempt(async () => (balances = await call('wallet/balances', { wallet: selected })))

  const verifyAudit = () => attempt(async () => (audit = await call('wallet/audit', { wallet: selected })))

  const newCosigner = () =>
    attempt(async () => {
      const created: any = await call('wallet/cosigner_new', { label: cosignerLabel.trim() })
      cosignerLabel = ''
      await copyText(JSON.stringify(created.intent))
      toast(() => t('twIntentCopied'))
    })

  onMount(() => {
    void reload()
    return onChange(['approval/changed', 'session/changed', 'wallet/changed'], reload)
  })

  /** An intent in a line. */
  function intentLine(intent: any): string {
    if (intent.kind === 'transfer' || intent.kind === 'batch') {
      const outputs = intent.outputs.map((o: any) => `${o.amount} → ${o.to}`).join(', ')
      return `${intent.asset.asset}${intent.asset.chain_id ? ` · ${intent.asset.chain_id}` : ''} ${outputs}`
    }
    return intent.kind
  }

  const approvalsOf = (p: any) => p.votes.filter((v: any) => v.decision === 'Approve').length
  const tone = (s: string) => (['Settled'].includes(s) ? 'ok' : ['Failed', 'Rejected', 'Vetoed', 'Expired', 'Stale', 'Cancelled'].includes(s) ? 'danger' : 'forest')
</script>

<Page wide testid="wallet-view">
  {#if error}<Notice tone="danger" testid="wallet-error">{error}</Notice>{/if}
  {#if !unlocked}
    <Notice>{t('locked')}</Notice>
  {:else}
    <section class="overflow-hidden rounded-2xl border border-line bg-surface shadow-xs">
      <div class="flex flex-wrap items-center gap-3 bg-gradient-to-br from-pale to-surface px-6 py-5">
        <span class="grid size-12 shrink-0 place-items-center rounded-2xl bg-forest text-on-forest [&_svg]:size-6"><Icon name="coins" /></span>
        <div class="min-w-0 flex-1">
          <p class="text-[12px] font-medium text-faint">{t('twNav')}</p>
          {#if wallets.length}
            <div class="mt-1 flex flex-wrap gap-1.5" data-testid="wallet-list">
              {#each wallets as w (w.wallet)}
                <button class={buttonClass(w.wallet === selected ? 'primary' : 'secondary', 'sm')} onclick={() => open(w.wallet)}>{w.name}</button>
              {/each}
            </div>
          {:else}
            <p class="text-[15px] text-muted">{t('twNone')}</p>
          {/if}
        </div>
        <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(reload)}><Icon name="sync-now" />{t('refresh')}</button>
      </div>
      {#if wallet}
        <dl class="divide-y divide-line border-t border-line px-6">
          <Fact label={t('twWallet')}><Id id={wallet.wallet} full /></Fact>
          <Fact label={t('role')}>{role || '—'}{#if wallet.frozen}<Pill tone="danger" class="ms-2">{t('twFrozen')}</Pill>{/if}</Fact>
          <Fact label={t('twPolicy')}>{t('twPolicyLine', { approvals: wallet.policy.approver_threshold, admin: wallet.policy.admin_threshold, version: wallet.policy.version })}</Fact>
          <Fact label={t('twCycles')}>{wallet.stats ? units(wallet.stats.cycles, 12) : '—'} T</Fact>
        </dl>
      {/if}
    </section>

    {#if wallet}
      <Section title={t('twAddresses')} icon="key" flush testid="wallet-addresses">
        {#snippet actions()}
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={readBalances}>{t('twBalances')}</button>
        {/snippet}
        <ul class="divide-y divide-line border-t border-line">
          {#each wallet.addresses as a (a.id)}
            {@const read = balances?.addresses.find((b: any) => b.address === a.id)}
            <li class="flex flex-wrap items-start gap-3 px-5 py-3">
              <Pill>{a.id}</Pill>
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <b class="text-[13px] font-medium">{a.label || a.chain}</b>
                  <Pill>{a.chain}</Pill>
                  {#if a.control !== 'chain_key'}<Pill tone="gem">{a.control}</Pill>{/if}
                  {#if a.cosign}<Pill tone="gem">{t('twCosigned')}</Pill>{/if}
                  {#if a.verified}<Pill tone="ok"><Icon name="verified" class="size-3" />{t('verified')}</Pill>{:else}<Pill tone="danger">{t('twUnverified')}</Pill>{/if}
                </div>
                <p class="mt-1 font-mono text-[11.5px] break-all text-ink-2 select-all">{a.text}</p>
                {#if read}
                  <p class="mt-1 flex flex-wrap gap-x-3 text-[12px] text-muted tabular-nums">
                    {#each read.balances as b, i (i)}
                      <span>{b.asset}{b.chain_id ? ` ${b.chain_id}` : ''}: {b.balance.Ok ?? b.balance.Err ?? b.balance}</span>
                    {/each}
                  </p>
                {/if}
              </div>
              <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(a.text)}><Icon name="copy" /></button>
            </li>
          {/each}
        </ul>
      </Section>

      <Section title={t('twProposals')} icon="approvals" flush testid="wallet-proposals">
        {#snippet actions()}
          {#if decider && !wallet.frozen}<button class={buttonClass('danger', 'xs')} disabled={busy} onclick={freeze}>{t('freeze')}</button>{/if}
          <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={verifyAudit}>{t('twAudit')}</button>
        {/snippet}
        {#if audit}<p class="px-5 pb-2 text-[12px] text-ok" data-testid="wallet-audit">{t('twAuditOk', { n: audit.events })}</p>{/if}
        <ul class="divide-y divide-line border-t border-line">
          {#each proposals as p (p.id)}
            <li class="flex flex-wrap items-center gap-3 px-5 py-3" data-testid="wallet-proposal">
              <Pill>#{p.id}</Pill>
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <Pill tone={tone(p.status)}>{p.status}</Pill>
                  <span class="text-[12px] text-muted">{t('twApprovalsOf', { n: approvalsOf(p), m: p.requirement.approvals })}</span>
                  {#if p.ready_at && p.status === 'Delayed'}<span class="text-[12px] text-faint">{t('twReadyAt')} {when(p.ready_at)}</span>{/if}
                </div>
                <p class="mt-1 text-[12.5px] [overflow-wrap:anywhere]">{intentLine(p.intent)}</p>
                {#if p.signed.length}<p class="mt-0.5 font-mono text-[11px] break-all text-faint">{p.signed[p.signed.length - 1].txid}</p>{/if}
                {#if p.failure}<p class="mt-0.5 text-[12px] text-danger">{p.failure}</p>{/if}
              </div>
              <div class="flex flex-wrap gap-1.5">
                {#if p.status === 'Open' && decider}
                  <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => decide('wallet/approve', p.id)}>{t('approve')}</button>
                  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => decide('wallet/reject', p.id)}>{t('reject')}</button>
                {/if}
                {#if p.status === 'Delayed' && decider}
                  <button class={buttonClass('danger', 'xs')} disabled={busy} onclick={() => decide('wallet/veto', p.id)}>{t('twVeto')}</button>
                {/if}
                {#if ['Ready', 'Executing', 'Signed', 'Broadcast'].includes(p.status) || (p.status === 'Delayed' && p.ready_at && p.ready_at <= Date.now())}
                  <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => execute(p.id)}>{t('twExecute')}</button>
                {/if}
                {#if ['Signed', 'Broadcast'].includes(p.status)}
                  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => step('wallet/broadcast', p.id)}>{t('twBroadcast')}</button>
                  <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => step('wallet/settle', p.id)}>{t('twSettle')}</button>
                {/if}
                {#if ['Open', 'Delayed', 'Ready'].includes(p.status) && p.proposer === status?.account}
                  <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => step('wallet/cancel', p.id)}>{t('cancel')}</button>
                {/if}
              </div>
            </li>
          {:else}
            <li class="px-5 py-4 text-[13px] text-faint">{t('twNoProposals')}</li>
          {/each}
        </ul>
      </Section>

      {#if role && role !== 'Viewer' && role !== 'Recovery' && !wallet.frozen}
        <Section title={t('twSend')} description={t('twSendHint')} icon="send">
          <div class="grid gap-3 sm:grid-cols-2">
            <Field label={t('twFrom')}>
              <Select bind:value={from} options={wallet.addresses.map((a: any) => ({ value: String(a.id), label: `${a.id} · ${a.label || a.chain}` }))} testid="wallet-from" />
            </Field>
            <Field label={t('twAsset')}>
              <Select bind:value={asset} options={assets} placeholder={t('choose')} testid="wallet-asset" />
            </Field>
            <Field label={t('recipient')} class="sm:col-span-2"><input bind:value={to} class="font-mono text-[12.5px]" data-testid="wallet-to" /></Field>
            <Field label={t('twAmount')}><input bind:value={amount} inputmode="numeric" class="tabular-nums" data-testid="wallet-amount" /></Field>
            <Field label={t('twMaxFee')}><input bind:value={maxFee} inputmode="numeric" class="tabular-nums" /></Field>
          </div>
          <div class="mt-3 flex justify-end">
            <button class={buttonClass('primary')} disabled={busy || !to.trim() || !amount.trim() || !asset} onclick={send} data-testid="wallet-send">{t('twPropose')}</button>
          </div>
        </Section>
      {/if}

      <Section title={t('members')} description={t('twMembersAgentsHint')} icon="user" flush testid="wallet-members">
        {#snippet actions()}
          {#if unlocked}<button class={buttonClass('secondary', 'sm')} onclick={() => (access = true)} data-testid="wallet-agent-access"><Icon name="agents" />{t('agentAccessButton')}</button>{/if}
        {/snippet}
        <ul class="divide-y divide-line border-t border-line">
          {#each wallet.members as m (m.id)}
            <li class="grid gap-1 px-5 py-2.5">
              <div class="flex flex-wrap items-center gap-2">
                <Id id={m.id} />
                <Pill>{m.role}</Pill>
                {#if m.status !== 'Active'}<Pill tone="warn">{m.status}</Pill>{/if}
                <span class="text-[12.5px] text-muted">{m.label}</span>
                {#if me?.id === m.id && unlocked && (hosted.length || m.agent_ids.length)}
                  <span class="flex-1"></span>
                  <!-- Those listed stay choices too: they may be hosted on another device. -->
                  <button
                    class={buttonClass('ghost', 'xs')}
                    onclick={() => (delegating = Object.fromEntries([...m.agent_ids, ...hosted.map((a) => a.agent_id)].map((id) => [id, m.agent_ids.includes(id)])))}
                    data-testid="wallet-member-agents">{t('twMemberAgents')}</button
                  >
                {/if}
              </div>
              {#if m.agent_ids?.length}
                <div class="flex flex-wrap items-center gap-1 text-[11.5px] text-faint">
                  {t('twProposesFor')}
                  {#each m.agent_ids as id (id)}<Pill class="font-mono font-normal">{agentName(id)}</Pill>{/each}
                </div>
              {/if}
              {#if me?.id === m.id && delegating}
                <div class="mt-1 grid gap-2 rounded-lg border border-line bg-surface-2 p-2.5">
                  <div class="flex flex-wrap gap-3">
                    {#each Object.keys(delegating) as id (id)}
                      <label class="inline-flex items-center gap-1.5 text-[12.5px]">
                        <input type="checkbox" bind:checked={() => delegating?.[id] ?? false, (on) => delegating && (delegating[id] = on)} />{agentName(id)}
                      </label>
                    {/each}
                  </div>
                  <p class="text-[11.5px] text-faint">{t('twMemberAgentsHint')}</p>
                  <div class="flex justify-end gap-2">
                    <button class={buttonClass('ghost', 'sm')} onclick={() => (delegating = null)}>{t('cancel')}</button>
                    <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={proposeAgents} data-testid="wallet-member-agents-propose">{t('twPropose')}</button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      </Section>
      <AgentAccess
        bind:open={access}
        title={t('walletAgentAccess', { name: wallet.name })}
        description={t('walletAgentAccessHint')}
        testid="wallet-access"
        choices={[
          { capability: 'wallet.read', label: t('walletRead') },
          { capability: `wallet.propose:${wallet.wallet}`, label: t('walletPropose') },
          { capability: `wallet.execute:${wallet.wallet}`, label: t('walletExecute') }
        ]}
      />
    {/if}

    <Section title={t('twNew')} description={t('twNewHint')} icon="add" testid="wallet-new">
      {#if !preview}
        <div class="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-surface-2 px-4 py-3.5">
          <span class="grid size-10 shrink-0 place-items-center rounded-xl border border-line bg-surface text-forest"><Icon name="coins" /></span>
          <p class="min-w-0 flex-1 text-[12.5px] leading-relaxed text-muted">{t('twQuoteHint')}</p>
          <button class={buttonClass('secondary')} disabled={busy || quoteLoading} onclick={getQuote} data-testid="wallet-quote">
            <Icon name={quoteLoading ? 'pending' : 'coins'} class={quoteLoading ? 'animate-spin motion-reduce:animate-none' : ''} />
            {t('twQuote')}
          </button>
        </div>
      {:else}
        <div class="@container rounded-xl border border-line bg-surface-2 p-3.5" data-testid="wallet-quote-line">
          <div class="flex flex-wrap items-start gap-3">
            <span class="mt-0.5 grid size-10 shrink-0 place-items-center rounded-xl border border-line bg-surface text-forest"><Icon name="coins" /></span>
            <dl class="grid min-w-0 flex-1 grid-cols-2 gap-x-6 gap-y-2 @min-[520px]:grid-cols-3">
              <div class="min-w-0">
                <dt class="text-[11px] text-faint">{t('twActivation')}</dt>
                <dd class="text-[19px] leading-snug font-semibold tabular-nums">{units(preview.price_e8s, 8)}{'\u00a0'}<span class="text-[12px] font-normal text-muted">ICP</span></dd>
              </div>
              <div class="min-w-0">
                <dt class="text-[11px] text-faint">{t('twFee')}</dt>
                <dd class="text-[13.5px] font-medium tabular-nums">{units(preview.ledger_fee_e8s, 8)}{'\u00a0'}<span class="text-[12px] font-normal text-muted">ICP</span></dd>
              </div>
              <div class="min-w-0">
                <dt class="text-[11px] text-faint">{t('twTotal')}</dt>
                <dd class="text-[13.5px] font-semibold tabular-nums">{units(total, 8)}{'\u00a0'}<span class="text-[12px] font-normal text-muted">ICP</span></dd>
              </div>
            </dl>
            <div class="flex items-center gap-1">
              <Pill tone={remaining > 0 ? 'forest' : 'warn'}>{t('twQuotaLeft', { left: preview.remaining, plan: preview.plan })}</Pill>
              <Tooltip content={t('refresh')}>
                {#snippet children(props)}
                  <button {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('refresh')} disabled={busy || quoteLoading} onclick={getQuote} data-testid="wallet-quote">
                    <Icon name="sync-now" class={quoteLoading ? 'animate-spin motion-reduce:animate-none' : ''} />
                  </button>
                {/snippet}
              </Tooltip>
            </div>
          </div>
          {#if remaining === 0}
            <p class="mt-3 flex flex-wrap items-center gap-1.5 border-t border-line pt-2.5 text-[12px] text-warn">
              <Icon name="warning" class="size-3.5" />
              <span>{t('twQuoteNone')}</span>
              <a class="font-medium text-forest underline underline-offset-2" href="#/plan">{t('openPlan')}</a>
            </p>
          {/if}
        </div>

        {#if remaining > 0}
          <div class="mt-4 grid gap-5" data-testid="wallet-new-form">
            <Field label={t('twName')}>
              <input bind:value={name} placeholder={t('twNameHint')} maxlength="64" autocomplete="off" data-testid="wallet-name" />
            </Field>

            <div class="grid gap-2">
              <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="flex items-baseline gap-2">
                  <span class="text-[12px] text-faint">{t('members')}</span>
                  <span class="text-[11px] text-faint tabular-nums">{memberCount}</span>
                </div>
                <button
                  type="button"
                  class={buttonClass('secondary', 'xs')}
                  disabled={busy || drafts.length >= MAX_DRAFT_MEMBERS}
                  onclick={addMember}
                  data-testid="wallet-member-add"
                >
                  <Icon name="login" />{t('twAddMember')}
                </button>
              </div>
              <div class="@container overflow-hidden rounded-xl border border-line" data-testid="wallet-members">
                <div class="flex flex-wrap items-center gap-2 border-b border-line bg-pale/40 px-3 py-2.5" data-testid="wallet-creator">
                  {#if creator}<Avatar id={creator} size="sm" />{/if}
                  <div class="min-w-0 flex-1">
                    <div class="flex flex-wrap items-center gap-1.5">
                      <b class="text-[12.5px] font-medium">{t('you')}</b>
                      <Pill tone="forest">{t('twRoleAdmin')}</Pill>
                    </div>
                    {#if creator}<div class="mt-0.5"><Id id={creator} /></div>{/if}
                  </div>
                </div>
                <div class="hidden gap-2 border-b border-line bg-surface-2 px-2.5 py-1.5 text-[11px] font-medium text-faint @min-[560px]:grid @min-[560px]:grid-cols-[minmax(0,1fr)_10rem_10rem_1.75rem]">
                  <span>{t('twMemberId')}</span>
                  <span>{t('role')}</span>
                  <span>{t('label')}</span>
                  <span></span>
                </div>
                {#each drafts as row, i (row.key)}
                  {@const problem = problems[i]}
                  <div
                    class="grid gap-2 border-b border-line p-2.5 last:border-0 @min-[560px]:grid-cols-[minmax(0,1fr)_10rem_10rem_1.75rem] @min-[560px]:items-start"
                    data-testid="wallet-member"
                  >
                    <input
                      bind:value={row.id}
                      class="min-w-0 font-mono text-[12.5px] placeholder:font-sans"
                      placeholder={t('twMemberId')}
                      spellcheck="false"
                      autocomplete="off"
                      aria-label={t('twMemberId')}
                      aria-invalid={problem ? 'true' : undefined}
                      onblur={() => (row.touched = true)}
                      data-testid="wallet-member-id"
                    />
                    <Select bind:value={row.role} options={roleOptions} label={t('role')} testid="wallet-member-role" class="w-full" />
                    <input
                      bind:value={row.label}
                      class="min-w-0"
                      placeholder={t('label')}
                      maxlength="64"
                      aria-label={t('label')}
                      data-testid="wallet-member-label"
                    />
                    <button
                      type="button"
                      class={buttonClass('ghost', 'sm', 'size-7 justify-self-start px-0')}
                      aria-label={t('twRemoveMember')}
                      disabled={busy}
                      onclick={() => removeMember(row.key)}
                      data-testid="wallet-member-remove"><Icon name="close" /></button
                    >
                    {#if problem && (row.touched || problem !== 'twMemberInvalid')}
                      <p class="text-[11.5px] text-danger @min-[560px]:col-span-4" data-testid="wallet-member-error">{t(problem)}</p>
                    {/if}
                  </div>
                {/each}
              </div>
              <p class="text-[11.5px] leading-normal text-faint">{t('twMembersHint')}</p>
            </div>

            <div class="@container grid gap-2">
              <span class="text-[12px] text-faint">{t('twPolicySection')}</span>
              <div class="grid gap-2 @min-[560px]:grid-cols-2">
                <div class="flex items-start gap-3 rounded-xl border border-line bg-surface-2 p-3.5">
                  <div class="min-w-0 flex-1">
                    <p class="text-[12.5px] font-medium">{t('twApprovals')}</p>
                    <p class="mt-0.5 text-[11.5px] leading-normal text-faint">{t('twApprovalsHint', { max: limits.approvers })}</p>
                  </div>
                  <div class="flex shrink-0 items-center rounded-lg border border-edge-soft bg-surface p-0.5">
                    <button
                      type="button"
                      class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                      aria-label={`${t('twDecrease')} ${t('twApprovals')}`}
                      disabled={busy || approvals <= 1}
                      onclick={() => (approvals -= 1)}
                      data-testid="wallet-approvals-dec"><span aria-hidden="true" class="text-[15px] leading-none">−</span></button
                    >
                    <span class="w-8 text-center text-[13px] font-medium tabular-nums" data-testid="wallet-approvals">{approvals}</span>
                    <button
                      type="button"
                      class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                      aria-label={`${t('twIncrease')} ${t('twApprovals')}`}
                      disabled={busy || approvals >= limits.approvers}
                      onclick={() => (approvals += 1)}
                      data-testid="wallet-approvals-inc"><span aria-hidden="true" class="text-[15px] leading-none">+</span></button
                    >
                  </div>
                </div>
                <div class="flex items-start gap-3 rounded-xl border border-line bg-surface-2 p-3.5">
                  <div class="min-w-0 flex-1">
                    <p class="text-[12.5px] font-medium">{t('twAdminApprovals')}</p>
                    <p class="mt-0.5 text-[11.5px] leading-normal text-faint">{t('twAdminApprovalsHint', { max: limits.admins })}</p>
                  </div>
                  <div class="flex shrink-0 items-center rounded-lg border border-edge-soft bg-surface p-0.5">
                    <button
                      type="button"
                      class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                      aria-label={`${t('twDecrease')} ${t('twAdminApprovals')}`}
                      disabled={busy || adminApprovals <= 1}
                      onclick={() => (adminApprovals -= 1)}
                      data-testid="wallet-admin-approvals-dec"><span aria-hidden="true" class="text-[15px] leading-none">−</span></button
                    >
                    <span class="w-8 text-center text-[13px] font-medium tabular-nums" data-testid="wallet-admin-approvals">{adminApprovals}</span>
                    <button
                      type="button"
                      class={buttonClass('ghost', 'sm', 'size-7 px-0')}
                      aria-label={`${t('twIncrease')} ${t('twAdminApprovals')}`}
                      disabled={busy || adminApprovals >= limits.admins}
                      onclick={() => (adminApprovals += 1)}
                      data-testid="wallet-admin-approvals-inc"><span aria-hidden="true" class="text-[15px] leading-none">+</span></button
                    >
                  </div>
                </div>
              </div>
            </div>

            <div class="flex flex-wrap items-center gap-2 border-t border-line pt-4">
              <p class="min-w-0 flex-1 text-[11.5px] leading-normal text-faint">
                {t('twCreateSummary', { total: units(total, 8), approvals, admin: adminApprovals })}
              </p>
              <button type="button" class={buttonClass('ghost')} disabled={busy} onclick={cancelCreate} data-testid="wallet-create-cancel">{t('cancel')}</button>
              <button
                type="button"
                class={buttonClass('primary')}
                disabled={busy || quoteLoading || !canCreate}
                onclick={create}
                data-testid="wallet-create"><Icon name="coins" />{t('twCreate')}</button
              >
            </div>
          </div>
        {/if}
      {/if}
    </Section>

    {#if deposits.length}
      <Section title={t('twDeposits')} description={t('twDepositsHint')} icon="restore" flush testid="wallet-deposits">
        <ul class="divide-y divide-line border-t border-line">
          {#each deposits as d (d.quote_id)}
            <li class="flex flex-wrap items-center gap-3 px-5 py-3" data-testid="wallet-deposit">
              <Id id={d.quote_id} />
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <b class="text-[13px] font-medium tabular-nums">{t('twDepositAmount', { amount: units(d.refund_e8s, 8) })}</b>
                  {#if d.state === 'failed'}
                    <Pill tone="danger">{t('twDepositFailed')}</Pill>
                  {:else if d.state === 'expired'}
                    <Pill tone="warn">{t('twDepositExpired')}</Pill>
                  {:else}
                    <Pill>{t('twDepositUnused', { time: when(d.expires_at) })}</Pill>
                  {/if}
                </div>
                {#if d.reason}<p class="mt-0.5 text-[12px] [overflow-wrap:anywhere] text-faint">{d.reason}</p>{/if}
              </div>
              {#if d.refundable}
                <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => refund(d.quote_id)} data-testid="wallet-refund">{t('twRefund')}</button>
              {/if}
            </li>
          {/each}
        </ul>
      </Section>
    {/if}

    <Section title={t('twJoin')} description={t('twJoinHint', { principal })} icon="link">
      <div class="flex gap-2">
        <input bind:value={joining} placeholder={t('twWallet')} class="min-w-0 flex-1 font-mono text-[12.5px]" data-testid="wallet-join-id" />
        <button class={buttonClass('secondary')} disabled={busy || !joining.trim()} onclick={join}>{t('join')}</button>
      </div>
    </Section>

    <Section title={t('twCosigners')} description={t('twCosignerHint')} icon="key" flush>
      <ul class="divide-y divide-line border-t border-line">
        {#each cosigners as c (c.label)}
          <li class={cn('flex flex-wrap items-center gap-2 px-5 py-2.5')}>
            <b class="text-[13px] font-medium">{c.label}</b>
            <Pill tone={c.hardware ? 'ok' : 'warn'}>{c.backend}</Pill>
            <span class="font-mono text-[11px] break-all text-faint">{c.public_key}</span>
          </li>
        {/each}
      </ul>
      <div class="flex gap-2 px-5 py-3">
        <input bind:value={cosignerLabel} placeholder={t('label')} class="min-w-0 flex-1" />
        <button class={buttonClass('secondary')} disabled={busy || !cosignerLabel.trim()} onclick={newCosigner}>{t('twNewCosigner')}</button>
      </div>
    </Section>
  {/if}
</Page>
