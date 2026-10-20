<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { copyText } from '../../components/clipboard'
  import Collapsible from '../../components/Collapsible.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Fact from '../../components/page/Fact.svelte'
  import Field from '../../components/page/Field.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Page from '../../components/page/Page.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Section from '../../components/page/Section.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, bridge, call, errorText, onChange, single, units, when } from '../../rpc'
  import { startLogin } from '../../shell/login.svelte'
  import { acceptLink, callerLabels, canonicalName, covers, nameActionLabels, nameProblem, namePrice, nameState, nameStateLabels, PRICE_TIERS, tokenOf } from './plan'

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
  let errors = $state<Record<string, string>>({})
  let busy = $state(false)
  let now = $state(Date.now())
  let name = $state('')
  /** The registry's answer for the name looked up. */
  let found = $state<any>(null)
  /** The transfer dialog: the name, its target, and once offered the token. */
  let moving = $state<{ name: string; target: string; token?: string } | null>(null)
  /** The accept dialog's token or link. */
  let incoming = $state<string | null>(null)
  let blocks = $state<Record<string, string>>({})

  const problem = $derived(nameProblem(name))
  const wanted = $derived(canonicalName(name))
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
      errors[key] = errorText(e)
    }
  }

  const readOperations = single(() =>
    part('operations', async () => {
      operations = await call('handle/operations')
      now = Date.now()
    })
  )

  const reload = single(() =>
    Promise.all([
      part('mine', async () => {
        mine = await call('handle/mine')
      }),
      readOperations(),
      part('payer', async () => {
        payer = await call('payer/status')
      })
    ])
  )

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

  /** Taking, accepting and completing are decided in the approval window. */
  const approve = (request: any) => bridge(() => window.dmsg.openApproval(request.id))

  const lookup = () =>
    attempt(async () => {
      found = null
      found = await call('handle/price', { name: wanted })
    })

  const register = () => attempt(async () => approve(await call('handle/register', { name: found.name })))
  const claim = () => attempt(async () => approve(await call('handle/claim', { name: found.name })))

  /** Offering prepares the transfer and its token; nothing moves until the owner completes it. */
  const offer = () =>
    attempt(async () => {
      const done: any = await call('handle/transfer', { name: moving!.name, target: moving!.target.trim() })
      moving = { ...moving!, token: done.token }
      await readOperations()
    })

  const accept = () =>
    attempt(async () => {
      const request = await call('handle/accept', { token: tokenOf(incoming ?? '') })
      incoming = null
      await approve(request)
    })

  const complete = (op: any) => attempt(async () => approve(await call('handle/transfer_commit', { id: op.id })))

  const forget = (op: any) =>
    attempt(async () => {
      await call('handle/forget', { id: op.id })
      await readOperations()
    })

  const settle = (op: any) =>
    attempt(async () => {
      await call('handle/reconcile', { id: op.id, block: Number(blocks[op.id]) })
      await reload()
    })

  /** The link goes to the target in a conversation: copied here, pasted there. */
  async function send(target: string, token: string) {
    await copyText(acceptLink(token))
    moving = null
    go(`/channels?direct=${encodeURIComponent(target)}`)
  }

  const message = (owner: string, handle: string) => go(`/channels?direct=${encodeURIComponent(owner)}&name=${encodeURIComponent(`@${handle}`)}`)

  // A link's token opens the accept dialog; a name from another view is looked up.
  $effect(() => {
    const next = ask
    if (!next) return
    untrack(() => {
      ask = null
      if (next.accept) incoming = next.accept
      if (next.name) {
        name = next.name
        if (!nameProblem(name) && canonicalName(name)) void lookup()
      }
    })
  })

  onMount(() => {
    void reload()
    const off = onChange(['approval/changed', 'session/changed'], reload)
    const timer = setInterval(() => {
      now = Date.now()
      if (pending) void readOperations()
    }, 10_000)
    return () => {
      off()
      clearInterval(timer)
    }
  })

  const panda = (n: number) => new Intl.NumberFormat(locale()).format(n)
  const label = (labels: Record<string, any>, value: string) => (labels[value] ? t(labels[value]) : value)
</script>

<Page>
  <Section title={t('myNames')} description={t('myNamesHint')} icon="user" testid="my-names">
    {#snippet actions()}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => (incoming = '')} data-testid="name-accept"><Icon name="transfer" />{t('acceptName')}</button>
    {/snippet}
    {#if errors.mine}
      <Notice tone="danger">{errors.mine}</Notice>
    {:else if mine && !mine.length}
      <p class="text-[12.5px] text-faint" data-testid="no-names">{t('noNames')}</p>
    {:else if mine}
      <ul class="divide-y divide-line rounded-xl border border-line">
        {#each mine as owned (owned.name)}
          <li class="flex items-center gap-3 px-4 py-2.5" data-testid="my-name">
            <b class="min-w-0 flex-1 truncate text-[14px] font-semibold">@{owned.name}</b>
            <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => (moving = { name: owned.name, target: '' })} data-testid="name-transfer">
              <Icon name="transfer" />{t('moveName')}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </Section>

  <Section title={t('lookUpName')} description={t('namesHint')} icon="search" testid="name-lookup">
    <div class="grid gap-3">
      <div class="flex gap-2">
        <label class="relative min-w-0 flex-1">
          <span class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint">@</span>
          <input
            bind:value={name}
            class="ps-7"
            placeholder={t('name')}
            aria-invalid={problem ? 'true' : undefined}
            data-testid="handle-name"
            onkeydown={(e) => e.key === 'Enter' && wanted && !problem && lookup()}
          />
        </label>
        <button class={buttonClass('secondary')} disabled={busy || !wanted || !!problem} onclick={lookup} data-testid="handle-lookup"><Icon name="search" />{t('lookUp')}</button>
      </div>
      {#if problem}
        <p class="text-[12px] text-danger" data-testid="handle-problem">{t(problem)}</p>
      {:else if wanted}
        <p class="text-[12px] text-faint" data-testid="handle-tier">{t('namePriceHint', { n: wanted.length, price: panda(namePrice(wanted.length)) })}</p>
      {/if}
      <Collapsible testid="name-prices">
        {#snippet title()}<span class="text-[12px] text-muted">{t('namePrices')}</span>{/snippet}
        <dl class="divide-y divide-line">
          {#each PRICE_TIERS as tier (tier.lengths)}
            <Fact label={t('nameLength', { lengths: tier.lengths })}>{panda(tier.panda)} PANDA</Fact>
          {/each}
        </dl>
      </Collapsible>

      {#if found}
        <div class="grid gap-3 rounded-xl border border-line px-4 py-3" data-testid="handle-state">
          <div class="flex flex-wrap items-center gap-2">
            <b class="text-[15px] font-semibold">@{found.name}</b>
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
              <button class={buttonClass('secondary', 'sm')} onclick={() => message(found.owner, found.name)} data-testid="handle-message"><Icon name="channels" />{t('sendMessage')}</button>
            {:else if !found.owner && found.reserved && found.claim?.claimable}
              <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={claim} data-testid="handle-claim">{t('claim')}</button>
            {:else if !found.owner && !found.reserved}
              <button class={buttonClass('primary', 'sm')} disabled={busy || enough === false} onclick={register} data-testid="handle-register">{t('register')}</button>
            {/if}
          </div>
          {#if !found.owner && !found.reserved}
            <dl class="divide-y divide-line">
              <Fact label={t('price')} testid="handle-price">{units(found.price, 8)} PANDA</Fact>
              <Fact label={t('networkFee')}>{units(found.fee, 8)} PANDA</Fact>
              <Fact label={t('atMost')}>{units(found.most, 8)} PANDA</Fact>
            </dl>
            {#if enough === false}
              <Notice tone="warn" icon="warning" testid="handle-short">
                {t('pandaShort')}
                {#snippet action()}<button class={buttonClass('secondary', 'xs')} onclick={() => go('/plan?tab=payments')}>{t('openWallet')}</button>{/snippet}
              </Notice>
            {/if}
          {:else if !found.owner && found.claim?.quarantined}
            <Notice tone="warn" icon="warning" testid="handle-quarantined">{t('nameQuarantined')}</Notice>
          {:else if !found.owner && found.claim && !found.claim.claimable}
            <Notice tone="info" icon="info" testid="handle-holders">
              <p>{t('claimHint')}</p>
              <ul class="mt-1.5 flex flex-wrap gap-1.5">
                {#each found.claim.holders ?? [] as holder (holder)}<li><Id id={holder} /></li>{/each}
              </ul>
              <div class="mt-2 flex flex-wrap gap-1.5">
                <button class={buttonClass('secondary', 'xs')} onclick={() => startLogin(false, 'https://panda.fans')} data-testid="handle-login"><Icon name="login" />{t('loginPandaFans')}</button>
                <button class={buttonClass('ghost', 'xs')} onclick={() => startLogin(true, 'https://panda.fans')} data-testid="handle-bind">{t('bindPandaFans')}</button>
              </div>
            </Notice>
          {/if}
        </div>
      {/if}
    </div>
  </Section>

  <Section title={t('nameOperations')} icon="history" flush testid="name-operations-section">
    {#if errors.operations}
      <div class="px-5 pb-4"><Notice tone="danger">{errors.operations}</Notice></div>
    {:else if operations && !operations.length}
      <p class="px-5 pb-4 text-[12.5px] text-faint">{t('noNameOperations')}</p>
    {:else if operations}
      <ul class="divide-y divide-line border-t border-line" data-testid="name-operations">
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
                <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(op.token)} data-testid="name-copy-token"><Icon name="copy" />{t('copyToken')}</button>
                <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(acceptLink(op.token))} data-testid="name-copy-link"><Icon name="link" />{t('copyLink')}</button>
              {/if}
              {#if op.phase === 'prepared' || op.phase === 'rejected' || state === 'expired'}
                <button class={buttonClass(op.token ? 'danger' : 'ghost', 'xs')} disabled={busy} onclick={() => forget(op)} data-testid="name-forget">
                  {op.token ? t('cancelTransfer') : t('removeEntry')}
                </button>
              {/if}
            </div>
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-muted">
              {#if op.from && op.to}<Id id={op.from} /><span aria-hidden="true">→</span><Id id={op.to} />{/if}
              {#if op.total}<span class="tabular-nums">{units(op.total, 8)} PANDA</span>{/if}
              <span>{label(callerLabels, op.caller_role)}</span>
              <span class="text-faint">{ago(op.created_at_ms, now)}</span>
            </div>
            {#if state === 'offered'}
              <p class="text-[12px] text-faint">{t('offeredHint')}</p>
            {:else if state === 'committable'}
              <p class="text-[12px] text-forest" data-testid="name-deadline">{t('completeBefore', { time: when(op.accepted_until_ms) })}</p>
            {:else if state === 'waiting'}
              <div class="flex flex-wrap items-center gap-2">
                <p class="text-[12px] text-faint" data-testid="name-waiting">{t('waitingOwner', { time: when(op.until_ms) })}</p>
                {#if op.from}
                  <!-- The owner learns of the acceptance in a conversation, not only on this page. -->
                  <button class={buttonClass('ghost', 'xs')} onclick={() => go(`/channels?direct=${encodeURIComponent(op.from)}`)} data-testid="name-tell-owner"><Icon name="channels" />{t('sendMessage')}</button>
                {/if}
              </div>
            {:else if state === 'unknown' && op.action === 'Register'}
              <div class="flex items-center gap-1.5">
                <input class="h-7 w-36 text-[12px]" placeholder={t('block')} bind:value={blocks[op.id]} data-testid="name-block" />
                <button class={buttonClass('secondary', 'xs')} disabled={busy || !/^\d+$/.test(blocks[op.id] ?? '')} onclick={() => settle(op)}>{t('reconcile')}</button>
              </div>
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
  testid="transfer"
>
  {#if moving && !moving.token}
    <Field label={t('targetAccount')} hint={t('targetAccountHint')}>
      <input bind:value={moving.target} placeholder={t('targetAccount')} class="font-mono text-[12.5px] placeholder:font-sans" data-testid="transfer-target" />
    </Field>
  {:else if moving?.token}
    <div class="grid gap-3">
      <p class="rounded-lg bg-surface-2 p-2.5 font-mono text-[11.5px] break-all select-all" data-testid="transfer-token">{moving.token}</p>
      <div class="flex flex-wrap gap-1.5">
        <button class={buttonClass('secondary', 'sm')} onclick={() => copyText(moving!.token!)} data-testid="transfer-copy"><Icon name="copy" />{t('copyToken')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={() => copyText(acceptLink(moving!.token!))} data-testid="transfer-link"><Icon name="link" />{t('copyLink')}</button>
        <button class={buttonClass('secondary', 'sm')} onclick={() => send(moving!.target.trim(), moving!.token!)} data-testid="transfer-send"><Icon name="send" />{t('sendInConversation')}</button>
      </div>
    </div>
  {/if}
  {#snippet footer()}
    {#if moving?.token}
      <button class={buttonClass('primary')} onclick={() => (moving = null)} data-testid="transfer-done">{t('done')}</button>
    {:else}
      <button class={buttonClass('secondary')} onclick={() => (moving = null)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy || !moving?.target.trim()} onclick={offer} data-testid="transfer-offer">{t('createToken')}</button>
    {/if}
  {/snippet}
</Dialog>

<Dialog bind:open={() => incoming !== null, (open) => (incoming = open ? incoming : null)} title={t('acceptName')} description={t('acceptHint')} icon="transfer" testid="accept">
  <Field label={t('token')}>
    <textarea
      bind:value={() => incoming ?? '', (text) => (incoming = text)}
      rows="4"
      class="font-mono text-[12px] placeholder:font-sans"
      placeholder={t('acceptPlaceholder')}
      data-testid="accept-token"
    ></textarea>
  </Field>
  {#snippet footer()}
    <button class={buttonClass('secondary')} onclick={() => (incoming = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !tokenOf(incoming ?? '')} onclick={accept} data-testid="accept-continue">{t('continueToApprove')}</button>
  {/snippet}
</Dialog>
