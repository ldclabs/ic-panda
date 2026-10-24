<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Page from '../components/page/Page.svelte'
  import Section from '../components/page/Section.svelte'
  import Field from '../components/page/Field.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Id from '../components/page/Id.svelte'
  import Select from '../components/Select.svelte'
  import TabBar from '../components/page/TabBar.svelte'
  import { buttonClass, iconButtonClass } from '../components/ui'
  import { copyText } from '../components/clipboard'
  import { toast } from '../components/toast.svelte'
  import { call, bridge, single, when, units } from '../rpc'
  import { t, stateLabel } from '../i18n/index.svelte'
  import AgentAccess from './agents/AgentAccess.svelte'
  import Create from './wallet/Create.svelte'
  import Transfer from './wallet/Transfer.svelte'
  import Manage from './wallet/Manage.svelte'
  import ProposalDetail from './wallet/Proposal.svelte'
  import Operations from './wallet/Operations.svelte'
  import Intent from './wallet/Intent.svelte'
  import { w } from './wallet/text'
  import { WALLET_CREATION_PAUSED } from '../../shared/wallet-availability'
  import { walletError as errorText } from './wallet/errors'
  import {
    actions,
    approvalCount,
    amountText,
    assetIdentity,
    assetKey,
    cyclesText,
    finalStates,
    stateAt,
    validPrincipal,
    ViewScope,
    walletRoute,
    type Wallet,
    type Proposal,
    type Asset
  } from './wallet/model'
  let {
    status,
    route = '/wallet',
    go = (path: string) => (location.hash = path)
  }: { status: any; route?: string; go?: (path: string) => void } = $props()
  let wallets = $state<any[]>([]),
    wallet = $state<Wallet | null>(null),
    selected = $state(''),
    proposals = $state<Proposal[]>([]),
    next = $state<number | null>(null)
  let metadata = $state<Asset[]>([]),
    balances = $state<any>(null),
    audit = $state<any>(null),
    book = $state<any[]>([]),
    bookNext = $state<string | null>(null)
  let proposalsLoading = $state(false), proposalError = $state(''), assetError = $state('')
  let balanceBusy = $state(false), bookBusy = $state(false), auditBusy = $state(false)
  let balanceRevision = 0, bookRevision = 0, proposalRevision = 0
  let readCore: () => Promise<void> = async () => {}
  let readProposals: () => Promise<void> = async () => {}
  let readAssets: () => Promise<void> = async () => {}
  let reloadBook: () => Promise<void> = async () => {}
  let assetsKey = '', bookKey = ''
  const extrasKey = $derived(wallet ? `${wallet.wallet}:${wallet.policy.version}:${wallet.audit_head}` : '')
  let loading = $state(false),
    busy = $state(false),
    listLoading = $state(false),
    error = $state(''),
    balanceError = $state(''),
    bookError = $state(''),
    depositError = $state('')
  let deposits = $state<any[]>([]),
    principal = $state(''),
    now = $state(Date.now()),
    tab = $state('overview'),
    filter = $state('all'),
    find = $state('')
  let newOpen = $state(false),
    transferOpen = $state(false),
    transferBusy = $state(false),
    joinOpen = $state(false),
    joining = $state(''),
    freezeOpen = $state(false),
    access = $state(false),
    receive = $state<any>(null)
  let manage = $state<{ kind: string; target?: any; key: number } | null>(null),
    manageOpen = $state(false),
    detailOpen = $state(false),
    detailId = $state<number | null>(null)
  let activation = $state<any>(null)
  let pendingRead = false
  let pending = $state<any>(null),
    drafts = $state<Record<string, any>>({})
  const scope = new ViewScope()
  let epoch = 0,
    listEpoch = 0,
    account = '',
    active = true,
    manageKey = 0
  const unlocked = $derived(status?.state === 'unlocked')
  const member = $derived(wallet?.me)
  const mayPropose = $derived(member?.status === 'Active' && !['Viewer', 'Recovery'].includes(member?.role ?? ''))
  const mayMembers = $derived(member?.status === 'Active' && member?.role !== 'Viewer')
  const decider = $derived(member?.status === 'Active' && ['Admin', 'Approver'].includes(member?.role ?? ''))
  const usable = $derived(!!wallet && !loading && !busy && wallet.wallet === selected && unlocked)
  const tabs = $derived(['overview', 'proposals', 'assets', 'team', 'activity', 'settings'].map((value) => ({ value, label: w(value) })))
  const shown = $derived(
    proposals.filter(
      (p) => filter === 'all' || (filter === 'waiting' ? actions(p, wallet!, now).approve : actions(p, wallet!, now).execute)
    )
  )
  const waiting = $derived(wallet ? proposals.filter((p) => actions(p, wallet!, now).approve).length : 0)
  const fresh = (generation: number) => active && scope.current(generation) && unlocked && status?.account === account

  $effect(() => {
    const current = unlocked ? String(status.account) : ''
    if (current !== account)
      untrack(() => {
        account = current
        epoch = scope.next()
        listEpoch++
        principal = ''
        wallets = []
        wallet = null
        selected = ''
        proposals = []
        metadata = []
        balances = null
        audit = null
        book = []
        deposits = []
        pending = null
        activation = null
        drafts = {}
        error = ''
        depositError = ''
        detailOpen = false
        manageOpen = false
        newOpen = false
        transferOpen = false
        loading = false
        proposalsLoading = balanceBusy = bookBusy = auditBusy = false
        proposalError = assetError = ''
        loadList = single(readList); loadPrincipal = single(readPrincipal); loadDeposits = single(readDeposits)
        busy = false
        listLoading = false
        receive = null
        if (current) { void loadList(); void loadPrincipal(); void loadDeposits() }
      })
  })
  $effect(() => {
    if (!unlocked || !wallet || !mayPropose || wallet.frozen) transferOpen = false
  })
  $effect(() => {
    const parts = route.split('?')[0].split('/')
    const id = parts[2] ?? ''
    if (unlocked && validPrincipal(id) && id !== selected) untrack(() => void open(id))
    const proposal = parts[3] && /^\d+$/.test(parts[3]) ? Number(parts[3]) : null
    if (proposal !== null && Number.isSafeInteger(proposal)) {
      detailId = proposal
      detailOpen = true
      tab = 'proposals'
    }
  })

  async function readList() {
    if (!active || !unlocked) return
    const run = listEpoch, owner = account
    listLoading = true
    try {
      const list = await call<any[]>('wallet/list')
      if (!active || run !== listEpoch || owner !== account || !unlocked) return
      wallets = list
      if (!selected && wallets.length) {
        const wanted = route.split('/')[2]
        select(validPrincipal(wanted ?? '') ? wanted : wallets[0].wallet)
      }
    } catch (e) { if (active && run === listEpoch) error = errorText(e) }
    finally { if (active && run === listEpoch) listLoading = false }
  }
  async function readPrincipal() {
    if (!active || !unlocked) return
    const run = listEpoch
    const mine = await call<any>('wallet/principal').catch(() => null)
    if (active && run === listEpoch && unlocked && mine) principal = mine.principal
  }
  async function readDeposits() {
    if (!active || !unlocked) return
    const run = listEpoch
    try {
      const paid = await call<any[]>('wallet/deposits')
      if (active && run === listEpoch && unlocked) { deposits = paid; depositError = '' }
    } catch (e) { if (active && run === listEpoch) depositError = errorText(e) }
  }
  let loadList = single(readList), loadPrincipal = single(readPrincipal), loadDeposits = single(readDeposits)

  function select(id: string) {
    if (busy) return
    go(walletRoute(id))
    void open(id)
  }
  async function pages(id: string, count: number, run: number) {
    const rows: Proposal[] = []
    let before: number | null = null
    do {
      const page: any = await call('wallet/proposals', { wallet: id, before, limit: Math.min(100, count - rows.length) })
      if (!fresh(run)) return null
      rows.push(...page.proposals)
      before = page.next ?? null
    } while (before !== null && rows.length < count)
    return { proposals: rows, next: before }
  }
  function open(id: string) {
    if (id !== selected) {
      const run = scope.next()
      epoch = run
      selected = id
      wallet = null; proposals = []; next = null; metadata = []
      balances = null; audit = null; book = []; bookNext = null
      balanceError = bookError = proposalError = assetError = error = ''
      loading = true; proposalsLoading = true
      balanceBusy = bookBusy = auditBusy = false
      balanceRevision++; bookRevision++; proposalRevision++
      assetsKey = bookKey = ''
      manageOpen = false; transferOpen = false; receive = null; access = false
      readCore = single(async () => {
        if (!fresh(run)) return
        try {
          const shown = await call<Wallet>('wallet/show', { wallet: id })
          if (!fresh(run)) return
          if (wallet?.audit_head !== shown.audit_head) { balances = null; audit = null; balanceRevision++ }
          wallet = shown
          metadata = shown.policy.spendable_assets.map((asset: Asset) => metadata.find((known) => assetKey(known) === assetKey(asset)) ?? asset)
          drafts[id] ??= { from: String(shown.addresses[0]?.id ?? 0), asset: '', outputs: [{ to: '', amount: '' }], maxFee: '', memo: '' }
          error = ''
        } catch (e) { if (fresh(run)) error = errorText(e) }
        finally { if (fresh(run)) loading = false }
      })
      readProposals = single(async () => {
        if (!fresh(run)) return
        const revision = ++proposalRevision
        proposalsLoading = true
        try {
          const page = await pages(id, Math.max(50, proposals.length), run)
          if (fresh(run) && revision === proposalRevision && page) { proposals = page.proposals; next = page.next; proposalError = '' }
        } catch (e) { if (fresh(run) && revision === proposalRevision) proposalError = errorText(e) }
        finally { if (fresh(run) && revision === proposalRevision) proposalsLoading = false }
      })
      readAssets = single(async () => {
        if (!fresh(run) || !['overview', 'assets'].includes(tab)) return
        const version = wallet?.policy.version
        try {
          const result = await call<any>('wallet/assets', { wallet: id })
          if (fresh(run) && version === wallet?.policy.version) { metadata = result.assets; assetError = '' }
        } catch (e) { if (fresh(run)) assetError = errorText(e) }
      })
      reloadBook = single(async () => {
        if (fresh(run) && ['overview', 'assets'].includes(tab)) await readBook()
      })
    }
    void readProposals()
    return readCore()
  }
  $effect(() => {
    const key = extrasKey
    const visible = ['overview', 'assets'].includes(tab)
    if (key && visible) untrack(() => {
      const assets = `${selected}:${wallet?.policy.version}`
      if (assetsKey !== assets) { assetsKey = assets; void readAssets() }
      if (bookKey !== key) { bookKey = key; void reloadBook() }
    })
  })
  function refresh() {
    void loadList(); void loadDeposits()
    if (selected) { void open(selected); void readAssets(); void reloadBook() }
  }
  async function readBalances() {
    if (balanceBusy) return
    const revision = ++balanceRevision
    const run = epoch,
      id = selected
    balanceBusy = true
    balanceError = ''
    try {
      const result = await call('wallet/balances', { wallet: id })
      if (fresh(run) && selected === id && revision === balanceRevision) balances = result
    } catch (e) {
      if (fresh(run) && revision === balanceRevision) balanceError = errorText(e)
    } finally {
      if (fresh(run)) balanceBusy = false
    }
  }
  async function readBook(more = false) {
    const revision = ++bookRevision
    const run = epoch,
      id = selected
    bookBusy = true
    bookError = ''
    try {
      const result: any = await call('wallet/book', { wallet: id, after: more ? bookNext : null })
      if (fresh(run) && revision === bookRevision) {
        book = more ? [...book, ...result.entries] : result.entries
        bookNext = result.next
      }
    } catch (e) {
      if (fresh(run) && revision === bookRevision) bookError = errorText(e)
    } finally {
      if (fresh(run) && revision === bookRevision) bookBusy = false
    }
  }
  async function readAudit(before?: number) {
    if (auditBusy) return
    const head = wallet?.audit_head
    const run = epoch,
      id = selected
    auditBusy = true
    error = ''
    try {
      const result = await call('wallet/audit_page', { wallet: id, before })
      if (fresh(run) && head === wallet?.audit_head) audit = result
    } catch (e) {
      if (fresh(run)) error = errorText(e)
    } finally {
      if (fresh(run)) auditBusy = false
    }
  }
  async function more() {
    if (next === null || !usable || proposalsLoading) return
    const revision = ++proposalRevision
    const run = epoch
    proposalsLoading = true
    try {
      const page: any = await call('wallet/proposals', { wallet: selected, before: next, limit: 50 })
      if (fresh(run) && revision === proposalRevision) {
        proposals = [...proposals, ...page.proposals.filter((p: Proposal) => !proposals.some((old) => old.id === p.id))]
        next = page.next ?? null
      }
    } catch (e) {
      if (fresh(run) && revision === proposalRevision) proposalError = errorText(e)
    } finally {
      if (fresh(run) && revision === proposalRevision) proposalsLoading = false
    }
  }
  function showProposal(id: number) {
    detailId = id
    detailOpen = true
    go(walletRoute(selected, id))
  }
  function showManage(kind: string, target?: any) {
    if (!usable) return
    manage = { kind, target, key: manageKey++ }
    manageOpen = true
  }
  function proposed(p: any) {
    if (p.wallet && p.wallet !== selected) return
    if (p.notification_error) toast(() => w('notificationFailed'))
    void refresh()
    if (p.id != null) showProposal(p.id)
  }
  function request(request: any, owner: string) {
    if (owner !== account) return
    pending = request
    activation = null
    void readPending()
    newOpen = false
  }
  function dismissPending() {
    pending = null
    activation = null
  }
  async function readPending() {
    if (!pending || pendingRead) return
    pendingRead = true
    const requestId = pending.id
    const owner = account
    try {
      const r: any = await call('approval/show', { id: requestId })
      if (!active || !unlocked || owner !== account || pending?.id !== requestId) return
      pending = r
      if (r.detail?.action === 'create' && r.detail.quote_id) {
        const phase = await call('wallet/activation', { quote: r.detail.quote_id }).catch((e) => ({
          stage: 'unknown',
          reason: errorText(e)
        }))
        if (!active || !unlocked || owner !== account || pending?.id !== requestId) return
        activation = phase
      } else activation = null
      const completed = r.detail?.action === 'create'
        ? activation?.stage === 'ready' || (r.state === 'done' && r.detail?.result?.status === 'Ready')
        : r.state === 'done'
      if (completed) {
        const id = activation?.wallet ?? r.detail?.result?.wallet ?? r.detail?.wallet
        dismissPending()
        if (id && validPrincipal(id)) {
          joinOpen = false
          joining = ''
          select(id)
          void loadList()
        }
      }
    } catch (e) {
      if (owner === account && pending?.id === requestId) error = errorText(e)
    } finally {
      pendingRead = false
      if (active && unlocked && pending && pending.id !== requestId) void readPending()
    }
  }
  async function run(method: string, params: any = {}) {
    if (busy) return
    const owner = account,
      id = selected
    busy = true
    error = ''
    try {
      const result: any = await call(method, {
        ...(id && !['wallet/refund', 'wallet/activation_continue', 'wallet/activation_abort'].includes(method) ? { wallet: id } : {}),
        ...params
      })
      if (!active || !unlocked || owner !== account) return
      if (result.kind === 'wallet' && result.digest) {
        pending = result
        activation = null
        void readPending()
        await bridge(() => window.dmsg.openApproval(result.id))
      }
      if (method === 'wallet/activation_continue') {
        if (result.stage === 'ready' && validPrincipal(result.wallet ?? '')) {
          pending = null
          activation = null
          go(walletRoute(result.wallet))
          void open(result.wallet)
          void loadList()
        } else void readPending()
      } else if (method === 'wallet/receive' && id === selected) receive = result
      else if (method === 'wallet/refund') {
        toast(() => t('twRefunded', { to: result.to, block: result.block }))
        await loadDeposits()
      } else if (method === 'wallet/activation_abort') {
        await loadDeposits()
      } else if (method === 'wallet/freeze') {
        freezeOpen = false
        await open(id)
      }
    } catch (e) {
      if (owner === account) error = errorText(e)
    } finally {
      if (owner === account) busy = false
    }
  }
  onMount(() => {
    const off = window.dmsg.onEvent((event) => {
      if (!unlocked || !['resync', 'wallet/changed', 'approval/changed', 'channel/changed'].includes(event.type)) return
      void readPending()
      if (busy) return
      if (event.type === 'wallet/changed' || event.type === 'resync') { void loadList(); void loadDeposits() }
      if (selected) void open(selected)
    })
    const focused = () => {
      if (unlocked && !busy) void refresh()
    }
    window.addEventListener('focus', focused)
    const clock = setInterval(() => (now = Date.now()), 1000)
    const progress = setInterval(() => {
      if (
        unlocked &&
        pending &&
        (['executing', 'unknown'].includes(pending.state) ||
          (pending.state === 'done' && pending.detail?.action === 'create' && !['ready', 'failed', 'refunded'].includes(activation?.stage)) ||
          (pending.state === 'awaiting_user' && now >= pending.expires_at))
      )
        void readPending()
    }, 5000)
    const poll = setInterval(() => {
      if (unlocked && selected && !busy && !loading && document.visibilityState === 'visible') void open(selected)
    }, 30_000)
    return () => {
      active = false
      scope.clear()
      listEpoch++
      off()
      clearInterval(clock)
      clearInterval(poll)
      clearInterval(progress)
      window.removeEventListener('focus', focused)
    }
  })
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="wallet-view">
  {#if unlocked && wallet}
    <div class="flex flex-wrap items-center gap-3 px-8 py-3">
      <div class="min-w-0 flex-1" data-testid="wallet-list"
        ><Select
          value={selected}
          disabled={busy}
          onChange={select}
          options={wallets.map((a) => ({ value: a.wallet, label: `${a.name} · ${a.wallet.slice(0, 5)}` }))}
          label={t('twWallet')}
          testid="wallet-select"
        /><p data-testid="wallet-identity" class="text-faint mt-1 text-[11px]">{w(member?.role ?? 'unknown')} · {wallet.wallet}</p></div
      >
      <button class={iconButtonClass()} disabled={busy || listLoading || loading} onclick={refresh} aria-label={t('refresh')} title={t('refresh')}
        ><Icon name="sync-now" /></button
      >
      <div class="grid gap-0.5">
        <button class={buttonClass('ghost', 'sm')} disabled={busy || WALLET_CREATION_PAUSED} aria-describedby={WALLET_CREATION_PAUSED ? 'wallet-creation-status' : undefined} onclick={() => (newOpen = true)} data-testid="wallet-new-button"><Icon name="add" />{w('newWallet')}</button>
        {#if WALLET_CREATION_PAUSED}<span id="wallet-creation-status" class="max-w-56 text-[11px] text-faint" data-testid="wallet-creation-paused">{w('creationPausedHint')}</span>{/if}
      </div>
      <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => (joinOpen = true)}>{w('joinWallet')}</button>
      {#if mayPropose && !wallet.frozen}<button class={buttonClass('primary')} disabled={!usable} onclick={() => (transferOpen = true)} data-testid="wallet-transfer-open"><Icon name="send" />{w('send')}</button>{/if}
    </div>
    <TabBar bind:value={tab} items={tabs} label={t('twNav')} testid="wallet-tab" />
  {/if}
  <div class="min-h-0 flex-1">
    <Page wide>
      {#if !unlocked}<Notice>{t('locked')}</Notice>
      {:else}
        {#if error}<Notice tone="danger" testid="wallet-error"
            >{error}<button class={buttonClass('ghost', 'sm')} onclick={refresh}>{w('retry')}</button></Notice
          >{/if}
        {#if loading && !wallet}<p class="text-faint text-[12px]" role="status" data-testid="wallet-loading">{w('loading')}</p>{/if}
        {#if pending}<Notice testid="wallet-pending"
            ><span
              >{activation && (['executing', 'unknown', 'done'].includes(pending.state) || ['paid', 'creating', 'installing', 'ready', 'failed', 'refunded'].includes(activation.stage))
                ? w(`activation_${activation.stage}`)
                : stateLabel(pending.state)}</span
            >{#if pending.state === 'awaiting_user'}<button class={buttonClass('secondary', 'xs', 'ms-3')} onclick={() => bridge(() => window.dmsg.openApproval(pending.id))}
              >{w('openApproval')}</button
            >{/if}{#if pending.detail?.action === 'create' && ['unknown', 'failed', 'done'].includes(pending.state) && !['ready', 'refunded', 'failed'].includes(activation?.stage)}<button
                class={buttonClass('secondary', 'xs', 'ms-2')}
                disabled={busy}
                onclick={() => run('wallet/activation_continue', { request: pending.id })}>{w('continue')}</button
              >{/if}
            {#snippet action()}<button class={buttonClass('ghost', 'xs')} aria-label={t('close')} onclick={dismissPending}
                ><Icon name="close" /></button
              >{/snippet}</Notice
          >{/if}
        {#if wallet}
          {#if wallet.frozen}<Notice tone="warn"
              ><b>{w('frozen')}</b> · {w('freezeHint')}{#if mayPropose}<button
                  class={buttonClass('secondary', 'sm', 'mt-2')}
                  disabled={!usable}
                  onclick={() => showManage('unfreeze')}>{w('unfreeze')}</button
                >{/if}</Notice
            >{/if}
          {#if tab === 'overview'}
            <section class="grid grid-cols-3 gap-4 px-5" data-testid="wallet-metrics">
              <div class="min-w-0 py-2"
                ><p class="text-faint text-[12px]">{w('waiting')}</p><b class="text-[30px] font-semibold tabular-nums">{proposalsLoading && !proposals.length ? '—' : waiting}</b><button
                  class={buttonClass('ghost', 'xs', 'mt-2 block')}
                  onclick={() => {
                    tab = 'proposals'
                    filter = 'waiting'
                  }}>{w('details')}</button
                ></div
              >
              <div class="min-w-0 py-2"
                ><p class="text-faint text-[12px]">{w('cycles')}</p><b class="text-[30px] font-semibold whitespace-nowrap tabular-nums" data-testid="wallet-cycles"
                  >{cyclesText(wallet.stats?.cycles ?? '0')} <small class="text-[13px]">T</small></b
                ><button class={buttonClass('ghost', 'xs', 'mt-2 block')} onclick={() => (tab = 'settings')}>{w('settings')}</button></div
              >
              <div class="min-w-0 py-2"
                ><p class="text-faint text-[12px]">{w('policy')}</p><b class="text-[30px] font-semibold tabular-nums"
                  >{wallet.policy.approver_threshold}</b
                ><span class="text-muted ms-2 text-[12px]">{w('approvals')}</span><button
                  class={buttonClass('ghost', 'xs', 'mt-2 block')}
                  onclick={() => (tab = 'team')}>{w('editPolicy')}</button
                ></div
              >
            </section>
            <p class="text-faint -mt-5 px-5 text-[11px]">{w('activeHint')}</p>
            {#if proposalError}<Notice tone="danger">{proposalError}</Notice>{/if}
            <Section title={w('proposals')} icon="approvals" flush>
              {#each proposals.filter((p) => !finalStates.has(stateAt(p, now))).slice(0, 5) as p (p.id)}<button
                  class="border-line hover:bg-wash flex w-full items-center gap-3 border-t px-5 py-3 text-start"
                  disabled={!usable}
                  onclick={() => showProposal(p.id)}
                  ><Pill>#{p.id}</Pill><span class="min-w-0 flex-1 text-[13px]">{w(p.intent.kind)}</span><Pill>{w(stateAt(p, now))}</Pill
                  ></button
                >{:else}<p class="text-faint px-5 py-4 text-[13px]">{proposalsLoading ? w('loading') : t('twNoProposals')}</p>{/each}
            </Section>
            <Section title={w('assets')} icon="coins" testid="wallet-assets-summary">
              {#snippet actions()}<button class={buttonClass('ghost', 'sm')} onclick={() => (tab = 'assets')}>{w('details')}</button>{/snippet}
              <div class="flex flex-wrap gap-2">
                {#each wallet.addresses as address (address.id)}
                  <button class={buttonClass('ghost', 'sm')} onclick={() => (tab = 'assets')}><Icon name="wallet" />{address.label || address.chain}<span class="text-faint">{address.chain}</span>{#if !address.verified}<Pill tone="danger">{t('twUnverified')}</Pill>{/if}</button>
                {/each}
              </div>
              <p class="mt-2 text-[12px] text-muted">{metadata.map(assetIdentity).join(' · ') || w('noAssets')}</p>
              {#if assetError}<Notice tone="danger">{assetError}</Notice>{/if}
            </Section>
          {:else if tab === 'proposals'}
            <div class="flex flex-wrap items-center gap-2"
              ><Select
                bind:value={filter}
                options={['all', 'waiting', 'executable'].map((f) => ({ value: f, label: w(f) }))}
                label={w('proposals')}
              /><input
                class="min-w-0 flex-1"
                bind:value={find}
                inputmode="numeric"
                placeholder={w('findProposal')}
                aria-label={w('findProposal')}
              /><button
                class={buttonClass('secondary', 'sm')}
                disabled={!usable || !/^\d+$/.test(find) || !Number.isSafeInteger(Number(find))}
                onclick={() => showProposal(Number(find))}>{w('details')}</button
              >{#if decider && !wallet.frozen}<button
                  class={buttonClass('danger', 'sm')}
                  disabled={!usable}
                  onclick={() => (freezeOpen = true)}>{w('freeze')}</button
                >{/if}</div
            >
            {#if proposalError}<Notice tone="danger">{proposalError}</Notice>{/if}
            <Section title={w('proposals')} icon="approvals" flush testid="wallet-proposals">
              {#each shown as p (p.id)}<div
                  class="border-line flex flex-wrap items-start gap-3 border-t px-5 py-4"
                  data-testid="wallet-proposal"
                  ><Pill>#{p.id}</Pill><div class="min-w-0 flex-1"
                    ><div class="mb-2 flex flex-wrap gap-2"
                      ><Pill>{w(stateAt(p, now))}</Pill><span class="text-[12px]"
                        >{approvalCount(p, wallet.members)} / {p.requirement.approvals} · {w(
                          p.requirement.admin ? 'Admin' : 'Approver'
                        )}</span
                      >{#if actions(p, wallet, now).voted}<Pill>{w('voted')}</Pill>{/if}</div
                    ><Intent intent={p.intent} {metadata} compact /><p class="text-faint mt-2 text-[11px]"
                      >{when(p.created_at)} · {p.via_agent ?? p.proposer}</p
                    ></div
                  ><button class={buttonClass('secondary', 'sm')} disabled={!usable} onclick={() => showProposal(p.id)}
                    >{w('details')}</button
                  ></div
                >{:else}<p class="text-faint px-5 py-4">{proposalsLoading ? w('loading') : t('twNoProposals')}</p>{/each}
            </Section>
            {#if next !== null}<button
                class={buttonClass('secondary', 'sm', 'justify-self-center')}
                disabled={!usable || proposalsLoading}
                onclick={more}
                data-testid="wallet-more">{w('more')}</button
              >{/if}
          {:else if tab === 'assets'}
            <Section title={w('assets')} icon="coins" flush testid="wallet-addresses">
              {#snippet actions()}<button class={buttonClass('secondary', 'sm')} disabled={!usable || balanceBusy} onclick={readBalances}
                  >{t('twBalances')}</button
                >{#if mayPropose}<button
                    class={buttonClass('primary', 'sm')}
                    disabled={!usable || wallet?.frozen}
                    onclick={() => showManage('address')}>{w('addAddress')}</button
                  >{/if}{/snippet}
              {#if assetError}<div class="px-5 py-3"><Notice tone="danger">{assetError}</Notice></div>{/if}
              {#if balanceError}<div class="px-5 py-3"><Notice tone="danger">{balanceError}</Notice></div>{/if}
              {#if balances}<p class="text-faint px-5 py-2 text-[11px]">{w('readAt')}: {when(balances.read_at)}</p>{/if}
              {#each wallet.addresses as a (a.id)}
                <div class="border-line grid gap-2 border-t px-5 py-4" data-testid={`wallet-address-${a.id}`}
                  ><div class="flex flex-wrap items-center gap-2"
                    ><b class="text-[14px]">{a.label || a.chain}</b><Pill
                      >{a.chain === 'Icp' ? 'ICP' : a.chain === 'Evm' ? 'EVM' : a.chain}</Pill
                    ><Pill tone={a.verified ? 'ok' : 'danger'}>{t(a.verified ? 'verified' : 'twUnverified')}</Pill>{#if a.cosign}<Pill
                        >{w('cosign')} {a.cosign.threshold}/{a.cosign.cosigners.length}</Pill
                      >{/if}<span class="flex-1"></span><button
                      class={buttonClass('ghost', 'xs')}
                      aria-label={`${t('copy')} ${a.label || a.chain}`}
                      onclick={() => copyText(a.text)}><Icon name="copy" /></button
                    ><button
                      class={buttonClass('secondary', 'xs')}
                      disabled={!usable || !a.verified}
                      onclick={() => run('wallet/receive', { address: a.id })}>{w('receive')}</button
                    >{#if mayPropose && a.chain === 'Solana' && !a.sol_nonce}<button class={buttonClass('secondary', 'xs')} disabled={!usable || wallet.frozen} onclick={() => showManage('sol_nonce', a)}>{w('init_sol_nonce')}</button>{/if}{#if mayPropose && a.control !== 'taproot_2of2'}<button class={buttonClass('ghost', 'xs')} disabled={!usable} onclick={() => showManage('cosign', a)}
                        >{w('bindCosign')}</button
                      >{/if}</div
                  ><p class="font-mono text-[12px] break-all select-all" dir="ltr">{a.text}</p>
                  {#each balances?.addresses.find((b: any) => b.address === a.id)?.balances ?? [] as b, i (i)}<div
                      class="bg-surface-2 rounded-lg p-3 text-[12px]"
                      ><p class="text-faint mb-1 break-all">{assetIdentity(b.metadata ?? b)}</p>{#if b.balance.Err}<p class="text-danger"
                          >{b.balance.Err}</p
                        >{:else}<b class="text-[16px] tabular-nums"
                          >{amountText(String(b.balance.Ok ?? b.balance), b.metadata ?? b, w('atomic'))}</b
                        >{/if}<p class="text-faint mt-1 text-[11px] break-all">{w('source')}: {b.source ?? '—'}</p></div
                    >{/each}</div
                >
              {/each}
            </Section>
            <Section title={w('book')} icon="user" flush>
              {#snippet actions()}<button class={buttonClass('secondary', 'sm')} disabled={!usable} onclick={() => readBook()}
                  >{t('refresh')}</button
                >{#if mayPropose}<button class={buttonClass('secondary', 'sm')} disabled={!usable} onclick={() => showManage('book')}
                    >{w('addBook')}</button
                  >{/if}{/snippet}
              {#if bookError}<Notice tone="danger">{bookError}</Notice>{/if}
              {#each book as b, i (i)}<div class="border-line flex flex-wrap items-center gap-2 border-t px-5 py-3"
                  ><div class="min-w-0 flex-1"
                    ><b class="text-[13px]">{b.label}</b><p class="font-mono text-[12px] break-all">{b.destination}</p><small
                      class="text-faint">{w('cooldown')}: {when(b.added_at + wallet.policy.address_book_cooldown_ms)}</small
                    ></div
                  ><button class={buttonClass('ghost', 'xs')} onclick={() => copyText(b.destination)}>{t('copy')}</button
                  >{#if mayPropose}<button
                      class={buttonClass('ghost', 'xs')}
                      disabled={!usable}
                      onclick={() => showManage('book_remove', b)}>{w('remove')}</button
                    >{/if}</div
                >{:else}<p class="text-faint px-5 py-4 text-[12px]">{w('none')}</p>{/each}
              {#if bookNext}<button class={buttonClass('secondary', 'sm', 'm-4')} disabled={!usable || bookBusy} onclick={() => readBook(true)}
                  >{w('more')}</button
                >{/if}
            </Section>
          {:else if tab === 'team'}
            <Section title={t('members')} icon="user" flush testid="wallet-members">
              {#snippet actions()}<button class={buttonClass('secondary', 'sm')} onclick={() => (access = true)}
                  >{t('agentAccessButton')}</button
                >{#if mayMembers}<button class={buttonClass('primary', 'sm')} disabled={!usable} onclick={() => showManage('member')}
                    >{w('addMember')}</button
                  >{/if}{/snippet}
              {#if member?.role === 'Recovery'}<Notice>{w('recoveryHint')}</Notice>{/if}
              {#each wallet.members as m (m.id)}<div class="border-line flex flex-wrap items-start gap-3 border-t px-5 py-4"
                  ><div class="min-w-0 flex-1"
                    ><div class="flex flex-wrap items-center gap-2"
                      ><b>{m.label}</b><Pill>{w(m.role)}</Pill><Pill>{m.kind === 'principal' ? 'Principal' : t('account')}</Pill><Pill
                        tone={m.status === 'Active' ? 'ok' : 'warn'}>{w(m.status)}</Pill
                      ></div
                    ><Id id={m.id} full />{#if m.principals?.length}<p class="text-faint mt-2 font-mono text-[11px] break-all"
                        >{w('principals')}: {m.principals.join(', ')}</p
                      >{/if}{#if m.agent_ids?.length}<p class="mt-1 text-[12px] break-all">{w('agent_ids')}: {m.agent_ids.join(', ')}</p
                      >{/if}</div
                  >{#if m.status === 'Invited'}<button
                      class={buttonClass('secondary', 'xs')}
                      onclick={() => copyText(`dmsg://wallet/${wallet!.wallet}`)}>{w('invite')}</button
                    >{/if}{#if mayMembers}<button
                      class={buttonClass('secondary', 'xs')}
                      disabled={!usable}
                      onclick={() => showManage('member', m)}>{w('editMember')}</button
                    ><button class={buttonClass('ghost', 'xs')} disabled={!usable} onclick={() => showManage('remove_member', m)}
                      >{w('remove')}</button
                    >{/if}</div
                >{/each}
            </Section>
            <Section title={w('policy')} icon="security"
              >{#snippet actions()}{#if mayPropose}<button
                    class={buttonClass('secondary', 'sm')}
                    disabled={!usable || wallet?.frozen}
                    onclick={() => showManage('policy')}>{w('editPolicy')}</button
                  >{/if}{/snippet}<Intent intent={{ kind: 'set_policy', policy: wallet.policy }} /></Section
            >
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
          {:else if tab === 'activity'}
            <Section title={w('activity')} description={w('verifiedHint')} icon="approvals" flush>
              {#snippet actions()}<button class={buttonClass('secondary', 'sm')} disabled={!usable || auditBusy} onclick={() => readAudit()}
                  >{t('twAudit')}</button
                >{#if audit}<button class={buttonClass('secondary', 'sm')} onclick={() => bridge(() => window.dmsg.saveReport(audit))}
                    >{w('export')}</button
                  >{/if}{/snippet}
              {#if audit}<p class="text-ok px-5 py-3 text-[12px]" data-testid="wallet-audit"
                  >{t('twAuditOk', { n: audit.events })} · {when(audit.read_at)}</p
                ><p class="text-faint px-5 pb-3 font-mono text-[11px] break-all">{audit.head}</p>{#each audit.recent as e (e.seq)}<div
                    class="border-line flex flex-wrap gap-3 border-t px-5 py-3 text-[12px]"
                    ><span>#{e.seq}</span><div class="min-w-0 flex-1"
                      ><b>{e.kind}</b><p class="text-faint break-all">{e.actor} · {when(e.at)}</p></div
                    >{#if e.proposal != null}<button class={buttonClass('ghost', 'xs')} onclick={() => showProposal(e.proposal)}
                        >#{e.proposal}</button
                      >{/if}</div
                  >{/each}{#if audit.next != null}<button
                    class={buttonClass('secondary', 'sm', 'm-4')}
                    disabled={!usable || auditBusy}
                    onclick={() => readAudit(audit.next)}>{w('more')}</button
                  >{/if}{/if}
            </Section>
          {:else if tab === 'settings'}{#key wallet.wallet}<Operations
                {wallet}
                onManage={showManage}
                onUpdated={() => void refresh()}
                onProposed={proposed}
              />{/key}{/if}
        {:else if !loading}
          <Section title={t('twNav')} description={listLoading ? w('loading') : error ? w('retry') : t('twNone')} icon="coins"
            ><div class="flex flex-wrap gap-2"
              >{#each wallets as entry (entry.wallet)}<button class={buttonClass('secondary', 'sm')} onclick={() => select(entry.wallet)}
                  >{entry.name}</button
                >{/each}<button class={buttonClass('secondary', 'sm')} disabled={listLoading} onclick={refresh}>{t('refresh')}</button></div
            ></Section
          >
          {#if selected && error.includes('FORBIDDEN')}<Notice
              >{w('accessHint')}<button
                class={buttonClass('secondary', 'sm', 'mt-2')}
                disabled={busy}
                onclick={() => {
                  joining = selected
                  joinOpen = true
                }}>{w('joinWallet')}</button
              ></Notice
            >{/if}
          <Create {status} onRequest={request} />
          <Section title={w('joinWallet')} description={t('twJoinHint', { principal })} icon="link"
            ><Field label={t('twWallet')}><input bind:value={joining} dir="ltr" data-testid="wallet-join-id" /></Field><button
              class={buttonClass('secondary', 'sm', 'mt-3')}
              disabled={busy || !validPrincipal(joining.trim())}
              onclick={() => run('wallet/join', { wallet: joining.trim() })}>{t('join')}</button
            ></Section
          >
        {/if}
        {#if depositError}<Notice tone="danger"
            >{w('newWallet')}: {depositError}<button class={buttonClass('ghost', 'xs')} onclick={() => loadDeposits()}>{w('retry')}</button
            ></Notice
          >{/if}
        {#if deposits.length}
          <Section title={t('twDeposits')} description={t('twDepositsHint')} icon="restore" flush testid="wallet-deposits">
            {#each deposits as d (d.quote_id)}<div
                class="border-line flex flex-wrap items-center gap-3 border-t px-5 py-3"
                data-testid="wallet-deposit"
                ><div class="min-w-0 flex-1"
                  ><Id id={d.quote_id} /><p>{t('twDepositAmount', { amount: units(d.refund_e8s, 8) })}</p><p class="text-faint text-[12px]"
                    >{['surplus', 'creating', 'installing'].includes(d.state) ? w(`deposit_${d.state}`) : d.state === 'failed'
                      ? t('twDepositFailed')
                      : d.state === 'expired'
                        ? t('twDepositExpired')
                        : t('twDepositUnused', { time: when(d.expires_at) })}</p
                  >{#if d.reason}<p class="text-danger text-[12px] break-all">{d.reason}</p>{/if}</div
                >{#if d.abortable}<button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => { if (window.confirm(w('abortActivationHint'))) void run('wallet/activation_abort', { quote: d.quote_id }) }}>{w('abortActivation')}</button>{/if}{#if d.refundable}<button
                    class={buttonClass('primary', 'sm')}
                    disabled={busy}
                    onclick={() => run('wallet/refund', { quote: d.quote_id })}
                    data-testid="wallet-refund">{t('twRefund')}</button
                  >{:else if now >= d.expires_at}<button class={buttonClass('secondary', 'sm')} onclick={() => loadDeposits()}
                    >{t('refresh')}</button
                  >{/if}</div
              >{/each}
          </Section>
        {/if}
      {/if}
    </Page>
  </div>
</div>
{#if unlocked}
  <Dialog bind:open={newOpen} title={w('newWallet')} icon="coins" wide
    >{#if newOpen}<Create {status} onRequest={request} />{/if}</Dialog
  >
  <Dialog bind:open={joinOpen} title={w('joinWallet')} description={t('twJoinHint', { principal })} icon="link"
    ><Field label={t('twWallet')}><input bind:value={joining} dir="ltr" /></Field>{#snippet footer()}<button
        class={buttonClass('primary')}
        disabled={busy || !validPrincipal(joining.trim())}
        onclick={() => run('wallet/join', { wallet: joining.trim() })}>{t('join')}</button
      >{/snippet}</Dialog
  >
  {#if wallet}
    <Dialog bind:open={transferOpen} dismissible={!transferBusy} title={w('send')} description={t('twSendHint')} icon="send" wide testid="wallet-transfer-dialog">
      {#if transferOpen && mayPropose && !wallet.frozen}
        {#key wallet.wallet}<Transfer {wallet} {metadata} {book} bind:busy={transferBusy} bind:draft={drafts[wallet.wallet]} onDone={(proposal) => { transferOpen = false; proposed(proposal) }} />{/key}
      {/if}
    </Dialog>
    <Dialog bind:open={freezeOpen} title={w('freeze')} description={w('freezeHint')} icon="lock" tone="danger"
      >{#snippet footer()}<button class={buttonClass('ghost')} onclick={() => (freezeOpen = false)}>{t('cancel')}</button><button
          class={buttonClass('danger')}
          disabled={!usable}
          onclick={() => run('wallet/freeze')}>{w('freeze')}</button
        >{/snippet}<Id id={wallet.wallet} full /></Dialog
    >
    {#if manage}{#key manage.key}<Manage
          bind:open={manageOpen}
          {wallet}
          {metadata}
          kind={manage.kind}
          target={manage.target}
          onDone={proposed}
          onSetup={() => { manageOpen = false; tab = 'settings' }}
        />{/key}{/if}
    {#if detailId !== null && detailOpen}{#key `${wallet.wallet}:${detailId}`}<ProposalDetail
          bind:open={detailOpen}
          {wallet}
          id={detailId}
          {metadata}
          {now}
          onUpdated={() => void open(selected)}
        />{/key}{/if}
    <Dialog
      bind:open={
        () => !!receive,
        (on) => {
          if (!on) receive = null
        }
      }
      title={w('receive')}
      icon="key"
      >{#if receive}<div class="grid justify-items-center gap-4"
          ><img src={receive.qr} alt={`${w('receive')} ${receive.text}`} class="size-48" /><p
            class="font-mono text-[12px] break-all"
            dir="ltr">{receive.text}</p
          ><p class="text-[12px]">{receive.chain} {receive.network}</p><button
            class={buttonClass('primary', 'sm')}
            onclick={() => copyText(receive.text)}>{t('copy')}</button
          >{#if receive.owner}<dl class="grid w-full gap-2 text-[12px] break-all"
              ><dt>Owner</dt><dd class="font-mono">{receive.owner}</dd><dt>Subaccount</dt><dd class="font-mono">{receive.subaccount}</dd><dt
                >AccountIdentifier</dt
              ><dd class="font-mono">{receive.account_identifier}</dd></dl
            >{/if}</div
        >{/if}{#snippet footer()}<button class={buttonClass('secondary')} onclick={() => (receive = null)}>{t('close')}</button
        >{/snippet}</Dialog
    >
  {/if}
{/if}
