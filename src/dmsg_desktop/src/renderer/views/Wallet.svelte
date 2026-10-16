<script lang="ts">
  import { onMount } from 'svelte'
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
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, code, units, when } from '../rpc'

  /**
   * Team wallets: one canister per team. Proposing, executing what members
   * approved, broadcasting and reading run here; creating, joining,
   * deciding and cosigning open the approval window. The wallet signs every
   * transaction; this device checks each plan with its own decoder first.
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
  let quote = $state<any>(null)
  let error = $state('')
  let busy = $state(false)
  // New wallet.
  let name = $state('')
  let members = $state('')
  let approvals = $state('1')
  let adminApprovals = $state('1')
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

  const unlocked = $derived(status?.state === 'unlocked')
  const me = $derived(wallet?.me)
  const role = $derived(me?.role ?? '')
  const decider = $derived(['Admin', 'Approver'].includes(role))

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
      toast(() => (code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`))
    } finally {
      busy = false
    }
  }

  async function load() {
    if (!unlocked) return
    try {
      wallets = await call('wallet/list')
      principal = (await call('wallet/principal')).principal
      cosigners = await call('wallet/cosigners')
      if (!selected && wallets.length) selected = wallets[0].wallet
      error = ''
      if (selected) await open(selected)
    } catch (e) {
      error = `${t('error')}: ${code(e)}`
    }
  }

  async function open(id: string) {
    selected = id
    balances = null
    audit = null
    try {
      wallet = await call('wallet/show', { wallet: id })
      proposals = (await call('wallet/proposals', { wallet: id, limit: 50 })).proposals
    } catch (e) {
      wallet = null
      proposals = []
      // A wallet listing this account but not joined yet reads nothing.
      error = code(e) === 'FORBIDDEN' ? t('twNotJoined') : `${t('error')}: ${code(e)}`
    }
  }

  const approve = async (request: any) => bridge(() => window.dmsg.openApproval(request.id))

  const getQuote = () => attempt(async () => (quote = await call('wallet/quote')))

  /** `<account or principal> <role> [label]` per line. */
  function parsedMembers() {
    return members
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const [id, memberRole = 'approver', ...label] = line.split(/\s+/)
        return { id, role: memberRole, label: label.join(' ') }
      })
  }

  const create = () =>
    attempt(async () => {
      await approve(
        await call('wallet/create', {
          quote: quote.quote_id,
          name: name.trim(),
          members: parsedMembers(),
          approvals: Number(approvals),
          admin_approvals: Number(adminApprovals)
        })
      )
      quote = null
    })

  const join = () => attempt(async () => approve(await call('wallet/join', { wallet: joining.trim() })))

  const decide = (method: string, proposal: number) => attempt(async () => approve(await call(method, { wallet: selected, proposal })))

  const send = () =>
    attempt(async () => {
      await call('wallet/send', { wallet: selected, from: Number(from), asset, to: to.trim(), amount: amount.trim(), max_fee: maxFee.trim() })
      to = ''
      amount = ''
      await open(selected)
    })

  const execute = (proposal: number) =>
    attempt(async () => {
      const result: any = await call('wallet/execute', { wallet: selected, proposal })
      // A plan this device cosigns waits in the approval window.
      if (result.kind === 'wallet' && result.digest) await approve(result)
      else if (result.broadcast_error) toast(() => `${t('twBroadcastFailed')}: ${result.broadcast_error}`)
      await open(selected)
    })

  const step = (method: string, proposal: number) =>
    attempt(async () => {
      await call(method, { wallet: selected, proposal })
      await open(selected)
    })

  const freeze = () =>
    attempt(async () => {
      await call('wallet/freeze', { wallet: selected })
      await open(selected)
    })

  const readBalances = () => attempt(async () => (balances = await call('wallet/balances', { wallet: selected })))

  const verifyAudit = () => attempt(async () => (audit = await call('wallet/audit', { wallet: selected })))

  const newCosigner = () =>
    attempt(async () => {
      const created: any = await call('wallet/cosigner_new', { label: cosignerLabel.trim() })
      cosignerLabel = ''
      cosigners = await call('wallet/cosigners')
      await copyText(JSON.stringify(created.intent))
      toast(() => t('twIntentCopied'))
    })

  onMount(() => {
    void load()
    return window.dmsg.onEvent((event) => {
      if (['approval/changed', 'session/changed', 'wallet/changed'].includes(event.type)) void load()
    })
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
        <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => attempt(load)}><Icon name="sync-now" />{t('refresh')}</button>
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

      <Section title={t('members')} icon="user" flush>
        <ul class="divide-y divide-line border-t border-line">
          {#each wallet.members as m (m.id)}
            <li class="flex flex-wrap items-center gap-2 px-5 py-2.5">
              <Id id={m.id} />
              <Pill>{m.role}</Pill>
              {#if m.status !== 'Active'}<Pill tone="warn">{m.status}</Pill>{/if}
              <span class="text-[12.5px] text-muted">{m.label}</span>
            </li>
          {/each}
        </ul>
      </Section>
    {/if}

    <Section title={t('twNew')} description={t('twNewHint')} icon="add">
      {#if !quote}
        <button class={buttonClass('secondary')} disabled={busy} onclick={getQuote} data-testid="wallet-quote">{t('twQuote')}</button>
      {:else}
        <Notice testid="wallet-quote-line">{t('twQuoteLine', { price: units(quote.price_e8s, 8), fee: units(quote.ledger_fee_e8s, 8), left: quote.remaining, plan: quote.plan })}</Notice>
        <div class="mt-3 grid gap-3 sm:grid-cols-2">
          <Field label={t('twName')} class="sm:col-span-2"><input bind:value={name} data-testid="wallet-name" /></Field>
          <Field label={t('members')} hint={t('twMembersHint')} class="sm:col-span-2">
            <textarea bind:value={members} rows="3" class="font-mono text-[12.5px]"></textarea>
          </Field>
          <Field label={t('twApprovals')}><input bind:value={approvals} inputmode="numeric" /></Field>
          <Field label={t('twAdminApprovals')}><input bind:value={adminApprovals} inputmode="numeric" /></Field>
        </div>
        <div class="mt-3 flex justify-end gap-2">
          <button class={buttonClass('ghost')} onclick={() => (quote = null)}>{t('cancel')}</button>
          <button class={buttonClass('primary')} disabled={busy || !name.trim()} onclick={create} data-testid="wallet-create">{t('twCreate')}</button>
        </div>
      {/if}
    </Section>

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
