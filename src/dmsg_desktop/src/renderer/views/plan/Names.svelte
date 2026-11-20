<script lang="ts">
  import { display, watchDisplays, type Mode } from '../../display.svelte'
  import { onMount, tick, untrack } from 'svelte'
  import { copyText } from '../../components/clipboard'
  import { rememberedPlain, rememberPlain } from '../../view-memory'
  import { isNoticeAccount } from '../../../shared/notifications'
  import { reviewApproval } from './approval'
  import Confirm from '../../components/Confirm.svelte'
  import TextArea from '../../components/TextArea.svelte'
  import Collapsible from '../../components/Collapsible.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import TextField from '../../components/TextField.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Page from '../../components/page/Page.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, call, errorText, onChange, single, units, when } from '../../rpc'
  import { startLogin } from '../../shell/login.svelte'
  import { acceptLink, callerLabels, canonicalName, covers, ledgerBlock, nameActionLabels, nameProblem, namePrice, nameState, nameStateLabels, PRICE_TIERS, tokenOf } from './plan'

  /**
   * Names: this account's own, a name looked up with what registering it
   * costs or who may claim it, and the operations on names under way.
   * Moving a name takes both accounts: the owner offers it and passes the
   * token on, the target accepts it in its own approval window, and the
   * owner completes it while that acceptance lasts, ten minutes.
   * Registering, claiming, accepting and completing are decided in the
   * approval window.
   */
  let { status, go, ask = $bindable(null) }: { status: any; go: (path: string) => void; ask?: { accept?: string; name?: string } | null } = $props()

  let mine = $state<any[] | null>(null)
  let operations = $state<any[] | null>(null)
  let payer = $state<any>(null)
  let security = $state<any>(null)
  let errors = $state<Record<string, string>>({})
  let busy = $state(false)
  let now = $state(Date.now())
  const searchKey = untrack(() => `plan.name-search:${status.account}:${status.session ?? ''}`)
  let searchForm: HTMLFormElement
  const priorSearch = rememberedPlain<{ name: string; query: string }>(searchKey)
  let name = $state(priorSearch?.name ?? '')
  let looking = $state(false)
  let alive = true
  let lookupVersion = 0
  let lookupQuery = priorSearch?.query ?? ''
  /** The registry's answer for the name looked up. */
  let found = $state<any>(null)
  /** The transfer dialog: the name, its target, and once offered the token. */
  let moving = $state<{ name: string; target: string; token?: string } | null>(null)
  /** The accept dialog's token or link. */
  let incoming = $state<string | null>(null)
  let forgetting = $state<any>(null)
  let transferTried = $state(false), acceptTried = $state(false)
  let targetForm = $state<HTMLFormElement>(), acceptForm = $state<HTMLFormElement>()
  let composeButton = $state<HTMLButtonElement>()
  let transferError = $state(''), acceptError = $state(''), lookupActionError = $state('')
  let payerLoading = $state(true)
  const recipient = $derived(moving?.target.trim().toLowerCase() ?? '')
  const targetProblem = $derived(!isNoticeAccount(recipient) ? t('planTargetError') : recipient === status?.account ? t('planOwnTarget') : '')
  let blocks = $state<Record<string, string>>({})

  const problem = $derived(nameProblem(name))
  const wanted = $derived(canonicalName(name))
  const holderBound = $derived((security?.logins ?? []).some((login: any) => found?.claim?.holders?.includes(login.principal)))
  /** Whether the payer's PANDA covers registering the name found; null while unknown. */
  const enough = $derived(found && !found.owner && !found.reserved ? covers(payer, found.ledger, found.most) : null)
  /** A transfer waiting on either account changes without this window, so it is read again. */
  const pending = $derived((operations ?? []).some((op) => ['offered', 'committable', 'waiting'].includes(nameState(op, now))))

  /** One part of the page: its failure shows in its place and leaves the others. */
  async function part(key: string, read: () => Promise<void>) {
    try {
      await read()
      errors[key] = ''
    } catch (e) {
      if (alive) errors[key] = errorText(e)
    }
  }

  const refreshOperations = single(() => part('operations', async () => {
    if (!alive) return
    const next = await call('handle/operations')
    if (alive) {
      const refreshed = new Map(next.map((op: any) => [op.id, op]))
      operations = (operations ?? next).map((op: any) => refreshed.get(op.id) ?? op)
      now = Date.now()
    }
  }))
  const readOperations = single(() => part('operations', async () => {
    if (!alive) return
    const cached = await call('handle/operations', { refresh: false })
    if (!alive) return
    // Preserve already fetched acceptance windows while reading local journals.
    const prior = new Map((operations ?? []).map((op) => [op.id, op]))
    operations = cached.map((op: any) => ({ ...prior.get(op.id), ...op }))
    now = Date.now()
    void refreshOperations()
  }))
  const readMine = single((mode: Mode = 'swr') => part('mine', async () => {
    if (!alive) return
    const next = await display('handle/mine', {}, mode)
    if (alive) mine = next
  }))
  const readPayer = single(async () => {
    if (!alive) return
    payerLoading = true
    await part('payer', async () => { const next = await call('payer/status'); if (alive) payer = next })
    if (alive) payerLoading = false
  })
  const readSecurity = single(() => part('security', async () => {
    if (!alive) return
    const next = await call('account/security')
    if (alive) security = next
  }))
  const refreshLookup = single(async () => {
    if (alive && lookupQuery && !looking) await readPrice(lookupQuery)
  })

  function reload() {
    return Promise.all([readPayer(), readSecurity(), refreshLookup(), readOperations(), readMine()])
  }

  async function attempt(work: () => Promise<unknown>, fail: (message: string) => void = message => toast(() => message)) {
    if (busy) return
    busy = true
    try {
      await work()
    } catch (e) {
      if (alive) fail(errorText(e))
    } finally {
      if (alive) busy = false
    }
  }

  function editName(next: string) {
    name = next
    lookupQuery = ''
    lookupVersion++
    found = null
    looking = false
    errors.lookup = lookupActionError = ''
  }

  async function readPrice(query: string) {
    const at = ++lookupVersion
    try {
      const next = await call('handle/price', { name: query })
      if (!alive || at !== lookupVersion) return
      found = next
      errors.lookup = ''
    } catch (e) {
      if (!alive || at !== lookupVersion) return
      found = null
      errors.lookup = errorText(e)
    } finally {
      if (alive && at === lookupVersion) looking = false
    }
  }

  async function lookup() {
    if (busy || looking || !wanted || problem) return
    found = null
    errors.lookup = ''
    looking = true
    lookupQuery = name.trim().replace(/^@/, '')
    await readPrice(lookupQuery)
  }

  function takeName(claim: boolean) {
    if (!found || busy || (!claim && (payerLoading || enough !== true || errors.payer))) return
    const query = lookupQuery, handle = claim ? found.name : found.display_handle ?? found.name
    const method = claim ? 'handle/claim' : 'handle/register'
    lookupActionError = ''
    return attempt(() => reviewApproval(`${method}:${handle}:${claim ? '' : found.most}`,
      () => call(method, { name: handle }), () => alive && lookupQuery === query),
      message => { if (lookupQuery === query) lookupActionError = message })
  }

  function openPayer() {
    rememberPlain(searchKey + '.focus', true)
    go('/plan?tab=payments&section=wallet')
  }
  function startTransfer(handle: string) {
    transferError = ''; transferTried = false
    moving = { name: handle, target: '' }
  }
  function startAccept(token = '') {
    acceptError = ''; acceptTried = false; incoming = token
  }
  function offer() {
    if (!moving || moving.token || busy) return
    transferTried = true
    if (targetProblem) { targetForm?.querySelector('input')?.focus(); return }
    const opened = moving, target = recipient
    transferError = ''
    return attempt(async () => {
      const done = await call<{ token: string }>('handle/transfer', { name: opened.name, target })
      if (!alive || moving !== opened) return
      moving = { ...opened, target, token: done.token }
      void readOperations()
      await tick()
      if (alive && moving?.token === done.token) composeButton?.focus()
    }, message => transferError = message)
  }
  function accept() {
    if (busy || incoming === null) return
    acceptTried = true
    const token = tokenOf(incoming)
    if (!token) { acceptForm?.querySelector('textarea')?.focus(); return }
    acceptError = ''
    return attempt(async () => {
      if (await reviewApproval('accept:' + token, () => call('handle/accept', { token }), () => alive && incoming !== null)) incoming = null
    }, message => acceptError = message)
  }
  const complete = (op: any) => attempt(() => reviewApproval('name-commit:' + op.id,
    () => call('handle/transfer_commit', { id: op.id }), () => alive), message => errors[op.id] = message)

  const forget = (op: any) =>
    attempt(async () => {
      await call('handle/forget', { id: op.id })
      await readOperations()
    })

  const settle = (op: any) =>
    attempt(async () => {
      const block = ledgerBlock(blocks[op.id])
      if (block === null) return
      errors[op.id] = ''
      await call('handle/reconcile', { id: op.id, block })
      await reload()
    }, message => errors[op.id] = message)

  function composeTransfer(target: string, token: string) {
    rememberPlain('plan.transfer-draft', { target, text: acceptLink(token) })
    moving = null
    go(`/channels?direct=${encodeURIComponent(target)}&compose=transfer`)
  }

  function message(owner: string, handle?: string) {
    const params = new URLSearchParams({ direct: owner, compose: 'message' })
    if (handle) params.set('name', `@${handle}`)
    go('/channels?' + params.toString())
  }

  // A link's token opens the accept dialog; a name from another view is looked up.
  $effect(() => {
    const next = ask
    if (!next) return
    untrack(() => {
      ask = null
      if (next.accept) startAccept(next.accept)
      if (next.name) {
        editName(next.name)
        if (!nameProblem(name) && canonicalName(name)) void lookup()
      }
    })
  })

  onMount(() => {
    if (rememberedPlain(searchKey + '.focus') && !ask) {
      rememberPlain(searchKey + '.focus', null)
      searchForm?.querySelector('input')?.focus()
    }
    const stop = watchDisplays(['handles'], (mode) => readMine(mode))
    void reload()
    const off = onChange(['approval/changed', 'session/changed'], reload)
    const timer = setInterval(() => {
      now = Date.now()
      if (pending) void readOperations()
    }, 10_000)
    return () => {
      rememberPlain(searchKey, { name, query: lookupQuery })
      alive = false
      off()
      stop()
      clearInterval(timer)
    }
  })

  const panda = (n: number) => new Intl.NumberFormat(locale()).format(n)
  const label = (labels: Record<string, any>, value: string) => (labels[value] ? t(labels[value]) : value)
</script>

<Page>
  <Section title={t('myNames')} description={t('myNamesHint')} icon="user" testid="my-names">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => startAccept()} data-testid="name-accept"><Icon name="transfer" />{t('acceptName')}</button>
    {/snippet}
    {#if errors.mine}
      <Notice failure={errors.mine} tone="danger">{errors.mine} <button class={buttonClass('ghost', 'xs')} onclick={() => readMine('fresh')}>{t('retry')}</button></Notice>
    {/if}
    {#if !mine && !errors.mine}<p class="text-xs text-muted" role="status">{t('loading')}</p>
    {:else if mine && !mine.length && !errors.mine}
      <p class="text-[12.5px] text-faint" data-testid="no-names">{t('noNames')}</p>
    {:else if mine}
      <ul class="divide-y divide-line">
        {#each mine as owned (owned.name)}
          <li class="flex items-center gap-3 px-4 py-2.5" data-testid="my-name">
            <b class="min-w-0 flex-1 truncate text-[14px] font-semibold">@{owned.display_handle ?? owned.name}</b>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} aria-label={`${t('moveName')} @${owned.display_handle ?? owned.name}`} onclick={() => startTransfer(owned.display_handle ?? owned.name)} data-testid="name-transfer">
              <Icon name="transfer" />{t('moveName')}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </Section>

  <Section title={t('lookUpName')} description={t('namesHint')} icon="search" testid="name-lookup">
    <div class="grid gap-3">
      <form bind:this={searchForm} class="flex items-end gap-2" onsubmit={(event) => { event.preventDefault(); void lookup() }}>
        <TextField class="min-w-0 flex-1" label={t('name')} value={name} placeholder="@…" dir="ltr"
          oninput={e => editName(e.currentTarget.value)}
          aria-invalid={!!problem} aria-describedby={problem ? 'handle-problem' : wanted ? 'handle-tier' : undefined} data-testid="handle-name" />
        <button class={buttonClass('secondary')} disabled={busy || looking || !wanted || !!problem} aria-busy={looking} type="submit" data-testid="handle-lookup"><Icon name={looking ? 'pending' : 'search'} class={looking ? 'motion-safe:animate-spin' : ''} />{t('lookUp')}</button>
      </form>
      {#if problem}
        <p id="handle-problem" class="text-[12px] text-danger" role="alert" data-testid="handle-problem">{t(problem)}</p>
      {:else if wanted}
        <p id="handle-tier" class="text-[12px] text-faint" data-testid="handle-tier">{t('namePriceHint', { n: wanted.length, price: panda(namePrice(wanted.length)) })}</p>
      {/if}
      {#if errors.lookup}<Notice failure={errors.lookup} tone="danger" testid="handle-lookup-error">{errors.lookup}</Notice>{/if}
      <Collapsible testid="name-prices">
        {#snippet title()}<span class="text-[12px] text-muted">{t('namePrices')}</span>{/snippet}
        <dl>
          {#each PRICE_TIERS as tier (tier.lengths)}
            <Fact label={t('nameLength', { lengths: tier.lengths })}>{panda(tier.panda)} PANDA</Fact>
          {/each}
        </dl>
      </Collapsible>

      {#if found}
        <div class="grid gap-3 rounded-lg bg-surface-2 px-4 py-3" data-testid="handle-state">
          <div class="flex flex-wrap items-center gap-2">
            <b class="text-[15px] font-semibold">@{found.display_handle ?? found.name}</b>
            {#if found.owner && found.owner === status?.account}
              <Pill tone="forest" testid="handle-yours">{t('yourName')}</Pill>
            {:else if found.owner}
              <Pill>{t('taken')}</Pill><Id id={found.owner} testid="handle-owner" />
            {:else if found.reserved}
              <Pill tone="gem">{t('reserved')}</Pill>
            {:else}
              <Pill tone="ok">{t('available')}</Pill>
            {/if}
            <span class="flex-1"></span>
            {#if found.owner && found.owner !== status?.account}
              <button class={buttonClass('secondary', 'sm')} onclick={() => message(found.owner, found.display_handle ?? found.name)} data-testid="handle-message"><Icon name="channels" />{t('sendMessage')}</button>
            {:else if !found.owner && found.reserved && !found.claim?.quarantined}
              {#if found.claim?.claimable}
                <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => takeName(true)} data-testid="handle-claim">{t('claim')}</button>
              {:else if holderBound}
                <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => startLogin(false, 'https://panda.fans')} data-testid="handle-login-claim"><Icon name="login" />{t('loginToClaim')}</button>
              {/if}
            {:else if !found.owner && !found.reserved}
              <button class={buttonClass('primary', 'sm')} disabled={busy || payerLoading || !!errors.payer || enough !== true} onclick={() => takeName(false)} data-testid="handle-register">{t('register')}</button>
            {/if}
          </div>
          {#if !found.owner && !found.reserved}
            <dl>
              <Fact label={t('price')} testid="handle-price">{units(found.price, 8)} PANDA</Fact>
              <Fact label={t('networkFee')}>{units(found.fee, 8)} PANDA</Fact>
              <Fact label={t('atMost')}>{units(found.most, 8)} PANDA</Fact>
            </dl>
            {#if payerLoading || enough === null || errors.payer}
              <Notice tone={payerLoading ? 'info' : 'warn'} testid="handle-balance-status">
                {payerLoading ? t('loading') : errors.payer || t('planBalanceUnknown')}
                {#if !payerLoading}<button class={buttonClass('secondary','xs')} onclick={readPayer}>{t('retry')}</button>{/if}
              </Notice>
            {:else if enough === false}
              <Notice tone="warn" icon="warning" testid="handle-short">
                {t('pandaShort')}
                {#snippet action()}<button class={buttonClass('secondary', 'xs')} onclick={openPayer}>{t('openWallet')}</button>{/snippet}
              </Notice>
            {/if}
          {:else if !found.owner && found.claim?.quarantined}
            <Notice tone="warn" icon="warning" testid="handle-quarantined">{t('nameQuarantined')}</Notice>
          {:else if !found.owner && found.claim && !found.claim.claimable}
            <Notice tone="info" icon="info" testid="handle-holders">
              <p>{t(holderBound ? 'claimBoundHint' : 'claimHint')}</p>
              <ul class="mt-1.5 flex flex-wrap gap-1.5">
                {#each found.claim.holders ?? [] as holder (holder)}<li><Id id={holder} /></li>{/each}
              </ul>
              {#if !holderBound}
                <div class="mt-2 flex flex-wrap gap-1.5">
                  <button class={buttonClass('secondary', 'xs')} onclick={() => startLogin(false, 'https://panda.fans')} data-testid="handle-login"><Icon name="login" />{t('loginPandaFans')}</button>
                  <button class={buttonClass('ghost', 'xs')} onclick={() => startLogin(true, 'https://panda.fans')} data-testid="handle-bind">{t('bindPandaFans')}</button>
                </div>
              {/if}
            </Notice>
          {/if}
          {#if lookupActionError}<Notice tone="danger" testid="name-action-error">{lookupActionError}</Notice>{/if}
          {#if found.reserved && errors.security}<Notice failure={errors.security} tone="danger">{errors.security}</Notice>{/if}
        </div>
      {/if}
    </div>
  </Section>

  <Section title={t('nameOperations')} icon="history" flush testid="name-operations-section">
    {#if errors.operations}
      <div class="px-5 pb-4"><Notice tone="warn">{errors.operations}
        <button class={buttonClass('ghost', 'xs')} onclick={readOperations}>{t('retry')}</button>
      </Notice></div>
    {/if}
    {#if !operations && !errors.operations}
      <p class="px-5 pb-4 text-xs text-muted" role="status">{t('loading')}</p>
    {:else if operations && !operations.length && !errors.operations}
      <p class="px-5 pb-4 text-[12.5px] text-faint">{t('noNameOperations')}</p>
    {:else if operations}
      <ul class="divide-y divide-line" data-testid="name-operations">
        {#each operations as op (op.id)}
          {@const state = nameState(op, now)}
          <li class="grid gap-1.5 px-5 py-3" data-testid="name-operation">
            <div class="flex flex-wrap items-center gap-2">
              <b class="text-[13px] font-medium">@{op.name}</b>
              <span class="text-[12.5px] text-muted">{label(nameActionLabels, op.action)}</span>
              <Pill tone={nameStateLabels[state][1]} testid="name-state">{t(nameStateLabels[state][0])}</Pill>
              <span class="flex-1"></span>
              {#if state === 'committable'}
                <button class={buttonClass('primary', 'xs')} disabled={busy} onclick={() => complete(op)} data-testid="name-complete">{t('commit')}</button>
              {/if}
              {#if op.token && (state === 'offered' || state === 'committable')}
                {#if op.to && state === 'offered'}<button class={buttonClass('secondary', 'xs')} onclick={() => composeTransfer(op.to, op.token)} data-testid="name-compose"><Icon name="channels" />{t('planComposeTransfer')}</button>{/if}
                <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(op.token)} data-testid="name-copy-token"><Icon name="copy" />{t('copyToken')}</button>
                <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(acceptLink(op.token))} data-testid="name-copy-link"><Icon name="link" />{t('copyLink')}</button>
              {/if}
              {#if op.phase === 'prepared' || op.phase === 'rejected' || state === 'expired'}
                <button class={buttonClass(op.token ? 'danger' : 'ghost', 'xs')} disabled={busy} onclick={() => op.token ? (forgetting = op) : forget(op)} data-testid="name-forget">
                  {op.token ? t('planRemoveTransfer') : t('removeEntry')}
                </button>
              {/if}
            </div>
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-muted">
              {#if op.from && op.to}<Id id={op.from} /><span aria-hidden="true">→</span><Id id={op.to} />{/if}
              {#if op.total}<span class="tabular-nums">{units(op.total, 8)} PANDA</span>{/if}
              <span>{label(callerLabels, op.caller_role)}</span>
              <span class="text-faint">{ago(op.created_at_ms, now)}</span>
            </div>
            {#if errors[op.id]}<Notice tone="danger">{errors[op.id]}</Notice>{/if}
            {#if state === 'offered'}
              <p class="text-[12px] text-faint">{t('offeredHint')}</p>
            {:else if state === 'committable'}
              <p class="text-[12px] text-forest" data-testid="name-deadline">{t('completeBefore', { time: when(op.accepted_until_ms) })}</p>
            {:else if state === 'waiting'}
              <div class="flex flex-wrap items-center gap-2">
                <p class="text-[12px] text-faint" data-testid="name-waiting">{t('waitingOwner', { time: when(op.until_ms) })}</p>
                {#if op.from}
                  <!-- The owner learns of the acceptance in a conversation, not only on this page. -->
                  <button class={buttonClass('ghost', 'xs')} onclick={() => message(op.from)} data-testid="name-tell-owner"><Icon name="channels" />{t('sendMessage')}</button>
                {/if}
              </div>
            {:else if state === 'unknown' && op.action === 'Register'}
              <form class="flex flex-wrap items-end gap-2" onsubmit={event => { event.preventDefault(); void settle(op) }}>
                <TextField class="w-44" inputmode="numeric" label={t('block')} bind:value={() => blocks[op.id] ?? '', value => blocks[op.id] = value}
                  error={blocks[op.id] && ledgerBlock(blocks[op.id]) === null ? t('wholeNumber') : undefined} data-testid="name-block" />
                <button type="submit" class={buttonClass('secondary', 'xs')} disabled={busy || ledgerBlock(blocks[op.id]) === null}>{t('reconcile')}</button>
              </form>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </Section>
</Page>

<Dialog
  bind:open={() => moving !== null, (open) => (moving = open ? moving : null)}
  title={t('transferNameOf', { name: `@${moving?.name ?? ''}` })}
  description={moving?.token ? t('transferHint') : t('transferStepsHint')}
  icon="transfer"
  tone="warn"
  dismissible={!busy}
  onOpenAutoFocus={(event) => { if (targetForm) { event.preventDefault(); targetForm.querySelector('input')?.focus() } }}
  testid="transfer"
>
  {#if moving && !moving.token}
    <form bind:this={targetForm} id="plan-transfer-target" onsubmit={(event) => { event.preventDefault(); void offer() }}>
      <TextField label={t('targetAccount')} hint={t('targetAccountHint')} bind:value={moving.target}
        disabled={busy} mono dir="ltr" spellcheck="false" autocomplete="off" error={transferTried ? targetProblem : ''}
        oninput={() => { transferError = ''; transferTried = false }} data-testid="transfer-target" />
      {#if transferError}<Notice tone="danger" class="mt-2" testid="transfer-error">{transferError}</Notice>{/if}
    </form>
  {:else if moving?.token}
    <div class="grid gap-3">
      <Fact label={t('targetAccount')}><Id id={moving.target} full wrap /></Fact>
      <div class="flex flex-wrap gap-2">
        <button bind:this={composeButton} class={buttonClass('primary', 'sm')} onclick={() => composeTransfer(moving!.target, moving!.token!)} data-testid="transfer-send"><Icon name="channels" />{t('planComposeTransfer')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={() => copyText(acceptLink(moving!.token!))} data-testid="transfer-link"><Icon name="link" />{t('copyLink')}</button>
      </div>
      <Collapsible testid="transfer-token-details">
        {#snippet title()}<span class="text-[12px]">{t('token')}</span>{/snippet}
        <p class="my-2 rounded-lg bg-surface-2 p-2.5 font-mono text-[11.5px] break-all select-all" dir="ltr" data-testid="transfer-token">{moving.token}</p>
        <button class={buttonClass('ghost', 'sm')} onclick={() => copyText(moving!.token!)} data-testid="transfer-copy"><Icon name="copy" />{t('copyToken')}</button>
      </Collapsible>
    </div>
  {/if}
  {#snippet footer()}
    {#if moving?.token}
      <span class="flex-1"></span><button class={buttonClass('secondary')} onclick={() => (moving = null)} data-testid="transfer-done">{t('done')}</button>
    {:else}
      <button class={buttonClass('secondary')} disabled={busy} onclick={() => (moving = null)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy} aria-busy={busy} type="submit" form="plan-transfer-target" data-testid="transfer-offer">{t('createToken')}</button>
    {/if}
  {/snippet}
</Dialog>

<Dialog bind:open={() => incoming !== null, (open) => (incoming = open ? incoming : null)}
  title={t('acceptName')} description={t('acceptHint')} icon="transfer" dismissible={!busy} testid="accept"
  onOpenAutoFocus={(event) => { event.preventDefault(); acceptForm?.querySelector('textarea')?.focus() }}>
  <form bind:this={acceptForm} id="plan-accept-token" onsubmit={(event) => { event.preventDefault(); void accept() }}>
    <TextArea label={t('token')} bind:value={() => incoming ?? '', text => { incoming = text; acceptError = ''; acceptTried = false }}
      disabled={busy} rows={4} mono dir="ltr" spellcheck="false" placeholder={t('acceptPlaceholder')}
      error={acceptTried && !tokenOf(incoming ?? '') ? t('planTokenError') : undefined} data-testid="accept-token" />
    {#if acceptError}<Notice tone="danger" class="mt-2" testid="accept-error">{acceptError}</Notice>{/if}
  </form>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('secondary')} disabled={busy} onclick={() => (incoming = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy} aria-busy={busy} type="submit" form="plan-accept-token" data-testid="accept-continue">{t('continueToApprove')}</button>
  {/snippet}
</Dialog>

<Confirm focusCancel bind:open={() => !!forgetting, open => { if (!open) forgetting = null }}
  title={t('planRemoveTransfer')} description={t('planRemoveTransferHint')} confirm={t('removeEntry')} testid="name-remove-dialog"
  onConfirm={async () => { await call('handle/forget', { id: forgetting.id }); if (alive) await readOperations() }}
  onReview={readOperations}>
  {#snippet body()}{#if forgetting}<b>@{forgetting.name}</b>{#if forgetting.to}<Fact label={t('targetAccount')}><Id id={forgetting.to} full wrap /></Fact>{/if}{/if}{/snippet}
</Confirm>
