<script lang="ts">
  import { daemonBlocked } from '../connection-gate'
  import { watchContent } from '../content-refresh'
  import Notice from '../components/page/Notice.svelte'
  import { Tabs } from 'bits-ui'
  import { onMount, untrack } from 'svelte'
  import { noticeVisibility } from '../notification-visibility'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import Avatar from '../components/Avatar.svelte'
  import { copyText } from '../components/clipboard'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import PaneHandle from '../components/PaneHandle.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { ago, call, closed, code, errorText, single, when } from '../rpc'
  import { shortId } from '../shell/session'
  import { remember, remembered } from '../view-memory'
  import { label } from './channels/names.svelte'
  import Compose from './inbox/Compose.svelte'
  import Rules from './inbox/Rules.svelte'

  /**
   * Letters in two columns: the inbox, sent letters and the archive beside
   * the open one. Writing a letter and the contact rules open in dialogs.
   */
  let { status, mod, route = '/inbox' }: { status: any; mod: string; route?: string } = $props()

  type Box = 'inbox' | 'sent' | 'archive'

  let letters = $state<any[]>([])
  let outgoing = $state<any[]>([])
  let outgoingLoaded = $state(false)
  let policy = $state<any>(null)
  let box = $state<Box>(remembered('inbox.box') ?? 'inbox')
  let selected = $state<string | null>(remembered('inbox.selected') ?? null)
  let query = $state('')
  let openedNotice = $state(''), missingNotice = $state(false)
  let orders = $state<Record<string, any>>({})
  let orderFailed = $state<Record<string, boolean>>({})
  let escrows = $state<Record<string, any>>({})
  let error = $state('')
  let refreshing = $state(false)
  let loaded = $state(false)
  let policyReady = $state(false)
  let generation = 0
  let alive = true
  let busy = $state(false)
  let writing = $state(false)
  let replyTo = $state('')
  let ruling = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)
  const valid = (at: number) => alive && at === generation && unlocked
  const inbox = $derived(letters.filter((l) => !l.archived).sort((a, b) => b.created_at - a.created_at))
  const archive = $derived(letters.filter((l) => l.archived).sort((a, b) => b.created_at - a.created_at))
  const unread = $derived(inbox.filter((l) => !l.read).length)
  const sent = $derived([...outgoing].sort((a, b) => (b.sent_at ?? 0) - (a.sent_at ?? 0)))
  const boxed = $derived(box === 'sent' ? sent : box === 'archive' ? archive : inbox)
  const asked = $derived(query.trim().toLowerCase())
  const list = $derived(asked ? boxed.filter(matches) : boxed)
  const ready = $derived(box === 'sent' ? outgoingLoaded : loaded)
  const letter = $derived(box === 'sent' ? null : list.find((l) => l.order_id === selected) ?? null)
  const order = $derived(box === 'sent' ? (outgoing.find((o) => o.order_id === selected) ?? null) : null)
  const wanted = $derived(new URLSearchParams(route.split('?')[1] ?? '').get('open'))
  const reportNotice = noticeVisibility(() => letter?.state === 'visible' && unlocked && loaded
    ? { account: status.account, session: status.session, target: { kind: 'inbox', id: letter.order_id } } : null,
    '[data-testid="letter"] > header')
  $effect(() => { void selected; void letters; void loaded; void status?.session; reportNotice() })

  const modeLabel: Record<string, Label> = { closed: 'closed', contacts: 'contactsOnly', free: 'free', paid: 'paidOnly' }
  const orderStates: Record<string, Label> = { quoted: 'orderQuoted', admission_committed: 'orderAdmitted', visible: 'orderDelivered', aborted: 'aborted' }
  const legStates: Record<string, Label> = {
    Pending: 'legPending',
    InFlight: 'legInFlight',
    Unknown: 'legUnknown',
    FeeBlocked: 'legFeeBlocked',
    Succeeded: 'legSucceeded',
    Rejected: 'legRejected',
    Superseded: 'legSuperseded'
  }
  const legKind = (kind: unknown) => t(kind === 'Recipient' ? 'legRecipient' : kind === 'Platform' ? 'legPlatform' : 'legRefund')
  const named = (state: string, names: Record<string, Label>) => (Object.hasOwn(names, state) ? t(names[state]) : state)

  function matches(item: any) {
    const account = box === 'sent' ? item.recipient : item.sender
    return [label(account), account, item.text ?? ''].some((text) => text.toLowerCase().includes(asked))
  }

  function write(to = '') {
    replyTo = to
    writing = true
  }

  async function attempt(work: () => Promise<unknown>) {
    const at = generation
    busy = true
    try {
      await work()
    } catch (e) {
      if (valid(at)) toast(() => errorText(e))
    } finally {
      if (valid(at)) busy = false
    }
  }

  const loadOutgoing = single(async () => {
    const at = generation
    if (!valid(at)) return
    try {
      const next = await call('inbox/outgoing')
      if (valid(at)) { outgoing = next; outgoingLoaded = true }
    } catch (e) {
      if (valid(at) && code(e) !== 'CACHE_MISSING' && !closed(e, status)) toast(() => errorText(e))
    }
  })

  const refreshLetters = single(async () => {
    const at = generation
    if (!valid(at) || document.visibilityState !== 'visible' || daemonBlocked()) return
    refreshing = true
    try {
      const next = await call('inbox/list')
      if (!valid(at)) return
      letters = next
      loaded = true
      error = ''
    } catch (e) {
      if (!valid(at)) return
      error = closed(e, status) ? 'LOCKED' : code(e) === 'RECOVERY_INCOMPLETE' ? 'RECOVERY_INCOMPLETE' : errorText(e)
      if (error === 'LOCKED') letters = outgoing = []
      else { const cached = await call('inbox/list', { refresh: false }).catch(() => null); if (valid(at) && cached) letters = cached }
    } finally {
      if (valid(at)) refreshing = false
    }
  })

  const loadPolicy = single(async (mode: Mode = 'swr') => {
    const at = generation
    if (!valid(at)) return
    try {
      const next = await display('inbox/policy', {}, mode)
      if (valid(at)) { policy = next; policyReady = true }
    } catch (e) {
      if (valid(at) && code(e) !== 'CACHE_MISSING' && !closed(e, status)) toast(() => errorText(e))
    }
  })

  const reload = single(async () => {
    const at = generation
    if (!valid(at)) return
    void loadOutgoing()
    try {
      const cached = await call('inbox/list', { refresh: false })
      if (!valid(at)) return
      letters = cached
      const observation = await call<{complete: boolean}>('inbox/status')
      if (!valid(at)) return
      loaded = cached.length > 0 || observation.complete
      // A cache read does not prove that a failed remote refresh recovered.
    } catch (e) {
      if (valid(at)) error = closed(e, status) ? 'LOCKED' : code(e) === 'RECOVERY_INCOMPLETE' ? 'RECOVERY_INCOMPLETE' : errorText(e)
    }
  })

  $effect(() => {
    void sessionKey
    untrack(() => {
      generation += 1
      letters = outgoing = []
      orders = escrows = orderFailed = {}
      policy = null
      loaded = outgoingLoaded = refreshing = policyReady = busy = writing = ruling = false
      error = unlocked ? '' : 'LOCKED'
      if (unlocked) { void reload().then(() => refreshLetters()); void loadPolicy() }
    })
  })

  // Keep a letter open: the first one when the open one left the list.
  $effect(() => {
    if (wanted && loaded && wanted !== openedNotice) {
      openedNotice = wanted
      const found = letters.find((item) => item.order_id === wanted)
      missingNotice = !found
      if (found) { box = found.archived ? 'archive' : 'inbox'; selected = wanted }
      else selected = null
    } else if (ready && !missingNotice && !list.some((l) => l.order_id === selected)) selected = list[0]?.order_id ?? null
  })

  $effect(() => {
    remember('inbox.box', box)
    remember('inbox.selected', selected)
  })

  // A sent letter says how it went as soon as it opens.
  $effect(() => {
    const target = order
    if (target) untrack(() => orders[target.order_id] || orderFailed[target.order_id] || check(target))
  })

  /** The daemon's `inbox/changed` reloads the letters. */
  const mark = (target: any, read: boolean, archived: boolean) => attempt(() => call('inbox/mark', { order: target.order_id, read, archived }))

  async function check(target: any) {
    const at = generation
    orderFailed[target.order_id] = false
    try {
      const next = await call('inbox/order', { recipient: target.recipient, order: target.order_id })
      if (valid(at)) orders[target.order_id] = next
    } catch {
      if (valid(at)) orderFailed[target.order_id] = true
    }
  }

  /** The escrow of a paid letter and its transfers; a refused one is resent at the fee the ledger asked for. */
  const funds = (target: any) =>
    attempt(async () => {
      const at = generation
      const next = await call('inbox/escrow', { order: target.order_id })
      if (valid(at)) escrows[target.order_id] = next
    })

  const revise = (target: any, leg: any) =>
    attempt(async () => {
      const at = generation
      await call('payment/revise', { escrow: leg.escrow_id, leg: leg.leg_id })
      if (!valid(at)) return
      const next = await call('inbox/escrow', { order: target.order_id })
      if (valid(at)) escrows[target.order_id] = next
    })

  onMount(() => {
    const stopContent = watchContent('inbox', () => unlocked ? 'inbox' : null, () => refreshLetters())
    const stop = watchDisplays(['inbox-policy'], (mode) => loadPolicy(mode))
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'approval/changed') { void loadOutgoing(); return }
      if (['inbox/changed', 'sync/changed', 'resync'].includes(event.type)) void reload()
      const detail = event.data as { method?: string; hint?: boolean } | null
      if (event.type === 'resync' || (event.type === 'inbox/changed' && (detail?.method === 'inbox/configure' || detail?.hint))) void loadPolicy()
    })
    return () => { alive = false; generation += 1; off(); stop(); stopContent() }
  })

  const tabClass =
    'flex h-7 flex-1 items-center justify-center gap-1.5 rounded-lg px-2 text-[12.5px] text-muted outline-none transition-colors duration-150 hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=active]:bg-surface data-[state=active]:font-medium data-[state=active]:text-ink'
</script>

{#snippet sender(account: string)}
  <bdi class="truncate text-[13px]" title={account}>{label(account)}</bdi>
{/snippet}

<div class="@container flex h-full min-h-0 flex-wrap" data-testid="inbox">
  {#if error === 'LOCKED'}
    <Locked {status} testid="inbox-locked" />
  {:else}
    <section class="flex h-[38%] min-h-0 w-full shrink-0 flex-col border-b border-line @min-[760px]:h-full @min-[760px]:w-[var(--pane,280px)] @min-[760px]:border-b-0 @min-[760px]:border-e @min-[1000px]:w-[var(--pane,320px)]">
      <div class="grid gap-2 p-3 pb-2">
        <div class="flex items-center gap-1.5">
          {#if unlocked}
            <button class={buttonClass('primary', 'md', 'flex-1')} onclick={() => write()} data-testid="letter-compose">
              <Icon name="edit" />{t('writeLetter')}
            </button>
          {/if}
          <Tooltip content={`${t('contactPolicy')} · ${policy ? t(modeLabel[policy.mode] ?? 'contactsOnly') : t('noPolicy')}`}>
            {#snippet children(props)}
              <button
                {...props}
                class={buttonClass('secondary', 'md', unlocked ? 'size-8 px-0' : 'flex-1')}
                aria-label={t('contactPolicy')}
                disabled={!unlocked || !policyReady}
                onclick={() => (ruling = true)}
                data-testid="contact-rules"
              >
                <Icon name="security" />{#if !unlocked}{t('contactPolicy')}{/if}
              </button>
            {/snippet}
          </Tooltip>
        </div>
        <Tabs.Root bind:value={() => box, (next) => (box = next as Box)}>
          <Tabs.List class="flex gap-0.5 rounded-[10px] bg-wash p-0.5">
            <Tabs.Trigger value="inbox" class={tabClass} data-testid="inbox-received">
              {t('inbox')}{#if unread}<span class="rounded-full bg-forest px-1.5 text-[10.5px] leading-4 font-semibold text-on-forest">{unread}</span>{/if}
            </Tabs.Trigger>
            <Tabs.Trigger value="sent" class={tabClass} data-testid="inbox-sent">{t('sentLetters')}</Tabs.Trigger>
            <Tabs.Trigger value="archive" class={tabClass}>{t('archived')}</Tabs.Trigger>
          </Tabs.List>
        </Tabs.Root>
        <label class="relative block">
          <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
          <input bind:value={query} class="ps-8" placeholder={t('searchLetters')} aria-label={t('searchLetters')} data-testid="letter-search" />
        </label>
      </div>
      {#if error}<Notice failure={error} tone="danger" testid="inbox-error">{error === 'RECOVERY_INCOMPLETE' ? t('syncInboxKeyMissing') : error}{#snippet action()}<button class={buttonClass('secondary', 'xs')} disabled={refreshing} onclick={() => refreshLetters()}>{t('retry')}</button>{/snippet}</Notice>{/if}
      <ul class="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="letters">
        {#each list as item (item.order_id)}
          {@const account = box === 'sent' ? item.recipient : item.sender}
          <li>
            <button
              class={cn(
                'grid w-full grid-cols-[auto_1fr_auto] items-start gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
                selected === item.order_id ? 'bg-pale' : 'hover:bg-wash'
              )}
              aria-current={selected === item.order_id ? 'true' : undefined}
              onclick={() => { missingNotice = false; selected = item.order_id }}
            >
              <Avatar id={account} />
              <span class="min-w-0">
                <span class={cn('flex items-center gap-1.5 truncate text-[13px] leading-snug', box !== 'sent' && !item.read ? 'font-semibold' : 'font-medium')}>
                  {#if box === 'sent'}<span class="text-faint">{t('to')}</span>{/if}
                  {@render sender(account)}
                </span>
                <span class="block truncate text-[12px] leading-snug text-faint">
                  {#if box === 'sent'}
                    {item.text ? item.text.split('\n')[0] : item.paid ? t('paidLetter') : t('freeLetter')}
                  {:else if item.state === 'visible'}
                    {item.text.split('\n')[0]}
                  {:else}
                    {t('aborted')}
                  {/if}
                </span>
              </span>
              <span class="flex flex-col items-end gap-1 pt-0.5 text-[11px] text-faint">
                {#if box !== 'sent' || item.sent_at}<span class="whitespace-nowrap">{ago(box === 'sent' ? item.sent_at : item.created_at)}</span>{/if}
                {#if box !== 'sent' && !item.read}<span class="size-2 rounded-full bg-forest" title={t('unread')}></span>{/if}
                {#if box === 'sent' && item.paid}<Icon name="coins" class="size-3.5 text-gem" />{/if}
              </span>
            </button>
          </li>
        {:else}
          <li class="grid justify-items-center gap-2 px-4 py-10 text-center">
            <span class="grid size-11 place-items-center text-faint [&_svg]:size-6"><Icon name="inbox" /></span>
            <p class="text-[13px] text-muted">{asked ? t('noResults') : !loaded && error ? t('serviceUnavailable') : !loaded || (refreshing && box !== 'sent') ? t('loading') : box === 'sent' ? t('noSent') : t('noLetters')}</p>
          </li>
        {/each}
      </ul>
    </section>
    <PaneHandle name="inbox" class="hidden @min-[760px]:block" />

    <section class="h-[62%] min-w-0 w-full overflow-y-auto @min-[760px]:h-full @min-[760px]:w-auto @min-[760px]:flex-1">
      {#if missingNotice}<div class="p-5"><Notice>{t('noticeEnded')}</Notice></div>{/if}
      {#if letter}
        <article class="mx-auto max-w-2xl px-4 py-5 @min-[900px]:px-8" data-testid="letter">
          <header class="flex flex-wrap items-start gap-3">
            <Avatar id={letter.sender} size="lg" />
            <div class="min-w-0 flex-1 basis-40">
              <button class="block max-w-full truncate text-start text-[13.5px] font-medium outline-none hover:text-forest focus-visible:underline" title={`${t('copy')} ${letter.sender}`} onclick={() => copyText(letter.sender)}>
                <bdi>{label(letter.sender)}</bdi>
              </button>
              <p class="text-[12px] text-faint">{when(letter.created_at)}</p>
            </div>
            {#if unlocked}
              <div class="flex w-full flex-wrap gap-1 @min-[1000px]:w-auto">
                {#if !letter.read}
                  <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => mark(letter, true, letter.archived)} data-testid="letter-read">
                    <Icon name="check" />{t('markRead')}
                  </button>
                {/if}
                <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => mark(letter, true, !letter.archived)} data-testid="letter-archive">
                  <Icon name={letter.archived ? 'unarchive' : 'archive'} />{letter.archived ? t('moveToInbox') : t('archive')}
                </button>
                <button class={buttonClass('primary', 'sm')} onclick={() => write(letter.sender)} data-testid="letter-reply">
                  <Icon name="reply" />{t('reply')}
                </button>
                <button class={buttonClass('secondary', 'sm')} onclick={() => (location.hash = `/channels?direct=${letter.sender}`)} data-testid="letter-converse">
                  <Icon name="channels" />{t('startConversation')}
                </button>
              </div>
            {/if}
          </header>
          <div class="mt-5 py-4">
            {#if letter.state === 'visible'}
              <p class="text-[14px] leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]">{letter.text}</p>
            {:else}
              <p class="flex items-center gap-2 text-[13px] text-muted"><Icon name="warning" class="text-warn" />{t('aborted')}{letter.abort_reason ? ` · ${letter.abort_reason}` : ''}</p>
            {/if}
          </div>
        </article>
      {:else if order}
        <article class="mx-auto grid max-w-2xl gap-4 px-4 py-5 @min-[900px]:px-8" data-testid="sent-letter">
          <header class="flex flex-wrap items-start gap-3">
            <Avatar id={order.recipient} size="lg" />
            <div class="min-w-0 flex-1 basis-40">
              <button class="block max-w-full truncate text-start text-[13.5px] font-medium outline-none hover:text-forest focus-visible:underline" title={`${t('copy')} ${order.recipient}`} onclick={() => copyText(order.recipient)}>
                <span class="text-faint">{t('to')}</span> <bdi>{label(order.recipient)}</bdi>
              </button>
              {#if order.sent_at}<p class="text-[12px] text-faint">{when(order.sent_at)}</p>{/if}
            </div>
            <span class={cn('rounded-md px-2 py-0.5 text-[11.5px] font-medium', order.paid ? 'bg-gem-pale text-gem' : 'bg-wash text-muted')}>
              {order.paid ? t('paidLetter') : t('freeLetter')}
            </span>
          </header>
          {#if order.text}
            <p class="py-2 text-[14px] leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]" data-testid="sent-text">{order.text}</p>
          {/if}
          <dl class="grid gap-1 border-t border-line pt-2">
            <div class="grid grid-cols-[120px_1fr] items-center gap-4 py-1.5">
              <dt class="text-[12px] text-faint">{t('status')}</dt>
              <dd class="flex items-center gap-2 text-[12.5px]">
                {#if orders[order.order_id]}
                  <span data-testid="order-state">{named(orders[order.order_id].state, orderStates)}</span>
                {:else if orderFailed[order.order_id]}
                  <span class="text-faint">{t('serviceUnavailable')}</span>
                  <button class={buttonClass('secondary', 'xs')} onclick={() => check(order)} data-testid="order-retry">{t('retry')}</button>
                {:else}
                  <span class="text-faint">{t('loading')}</span>
                {/if}
              </dd>
            </div>
            <div class="grid grid-cols-[120px_1fr] items-center gap-4 py-1.5">
              <dt class="text-[12px] text-faint">{t('order')}</dt>
              <dd class="truncate font-mono text-[12px]" title={order.order_id}>{order.order_id}</dd>
            </div>
          </dl>
          {#if order.paid}
            <div class="py-3">
              <div class="flex items-center justify-between gap-2">
                <b class="text-[13px] font-semibold">{t('funds')}</b>
                <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => funds(order)}>{t('checkFunds')}</button>
              </div>
              {#if escrows[order.order_id]}
                <ul class="mt-2 divide-y divide-line" data-testid="escrow-transfers">
                  {#each escrows[order.order_id].transfers as leg (leg.leg_id)}
                    <li class="flex items-center justify-between gap-2 py-2 text-[12.5px]">
                      <span class="min-w-0">
                        <span class="rounded bg-pale px-1.5 text-[11px] font-medium text-forest">{named(leg.status, legStates)}</span>
                        {legKind(leg.kind)} · {leg.amount} → <bdi dir="ltr" class="font-mono">{shortId(leg.to)}</bdi>
                      </span>
                      {#if ['FeeBlocked', 'Rejected'].includes(leg.status)}
                        <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => revise(order, leg)}>{t('revise')}</button>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/if}
        </article>
      {:else}
        <div class="grid h-full place-items-center p-8 text-center text-[13px] text-faint">
          <p>{list.length ? t('selectLetter') : ''}</p>
        </div>
      {/if}
    </section>
  {/if}
</div>

{#key sessionKey}
  <Compose bind:open={writing} to={replyTo} {mod} />
  <Rules bind:open={ruling} {policy} onSaved={(next) => (policy = next)} />
{/key}
