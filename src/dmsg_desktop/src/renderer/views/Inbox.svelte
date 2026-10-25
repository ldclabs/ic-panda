<script lang="ts">
  import Notice from '../components/page/Notice.svelte'
  import { Tabs } from 'bits-ui'
  import { onMount, untrack } from 'svelte'
  import { display, watchDisplays, type Mode } from '../display.svelte'
  import Avatar from '../components/Avatar.svelte'
  import { copyText } from '../components/clipboard'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t, type Label } from '../i18n/index.svelte'
  import { ago, call, closed, code, errorText, single, when } from '../rpc'
  import { shortId } from '../shell/session'
  import Compose from './inbox/Compose.svelte'
  import Rules from './inbox/Rules.svelte'

  /**
   * Letters in two columns: the inbox, sent letters and the archive beside
   * the open one. Writing a letter and the contact rules open in dialogs.
   */
  let { status, mod }: { status: any; mod: string } = $props()

  type Box = 'inbox' | 'sent' | 'archive'

  let letters = $state<any[]>([])
  let outgoing = $state<any[]>([])
  let policy = $state<any>(null)
  let box = $state<Box>('inbox')
  let selected = $state<string | null>(null)
  let orders = $state<Record<string, any>>({})
  let escrows = $state<Record<string, any>>({})
  let error = $state('')
  let refreshing = $state(false)
  let loaded = $state(false)
  let policyReady = $state(false)
  let generation = 0
  let alive = true
  let busy = $state(false)
  let writing = $state(false)
  let ruling = $state(false)

  const unlocked = $derived(status?.state === 'unlocked')
  const sessionKey = $derived(`${status?.account ?? ''}:${status?.state ?? ''}`)
  const valid = (at: number) => alive && at === generation && unlocked
  const inbox = $derived(letters.filter((l) => !l.archived).sort((a, b) => b.created_at - a.created_at))
  const archive = $derived(letters.filter((l) => l.archived).sort((a, b) => b.created_at - a.created_at))
  const unread = $derived(inbox.filter((l) => !l.read).length)
  const list = $derived(box === 'sent' ? outgoing : box === 'archive' ? archive : inbox)
  const letter = $derived(box === 'sent' ? null : list.find((l) => l.order_id === selected) ?? null)
  const order = $derived(box === 'sent' ? (outgoing.find((o) => o.order_id === selected) ?? null) : null)

  const modeLabel: Record<string, Label> = { closed: 'closed', contacts: 'contactsOnly', free: 'free', paid: 'paidOnly' }

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
      if (valid(at)) outgoing = next
    } catch (e) {
      if (valid(at) && code(e) !== 'CACHE_MISSING' && !closed(e, status)) toast(() => errorText(e))
    }
  })

  const refreshLetters = single(async () => {
    const at = generation
    if (!valid(at)) return
    refreshing = true
    try {
      const next = await call('inbox/list')
      if (!valid(at)) return
      letters = next
      loaded = true
      error = ''
    } catch (e) {
      if (!valid(at)) return
      error = closed(e, status) ? 'LOCKED' : errorText(e)
      if (error === 'LOCKED') letters = outgoing = []
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
      loaded = true
      error = ''
      void refreshLetters()
    } catch (e) {
      if (valid(at)) error = closed(e, status) ? 'LOCKED' : errorText(e)
    }
  })

  $effect(() => {
    void sessionKey
    untrack(() => {
      generation += 1
      letters = outgoing = []
      orders = escrows = {}
      selected = null
      policy = null
      loaded = refreshing = policyReady = busy = writing = ruling = false
      error = unlocked ? '' : 'LOCKED'
      if (unlocked) { void reload(); void loadPolicy() }
    })
  })

  // Keep a letter open: the first one when the open one left the list.
  $effect(() => {
    if (!list.some((l) => l.order_id === selected)) selected = list[0]?.order_id ?? null
  })

  /** The daemon's `inbox/changed` reloads the letters. */
  const mark = (target: any, read: boolean, archived: boolean) => attempt(() => call('inbox/mark', { order: target.order_id, read, archived }))

  const check = (target: any) =>
    attempt(async () => {
      const at = generation
      const next = await call('inbox/order', { recipient: target.recipient, order: target.order_id })
      if (valid(at)) orders[target.order_id] = next
    })

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
    const stop = watchDisplays(['inbox-policy'], (mode) => loadPolicy(mode))
    const off = window.dmsg.onEvent((event) => {
      if (event.type === 'approval/changed') { void loadOutgoing(); return }
      if (['inbox/changed', 'sync/changed', 'resync'].includes(event.type)) void reload()
      const detail = event.data as { method?: string; hint?: boolean } | null
      if (event.type === 'resync' || (event.type === 'inbox/changed' && (detail?.method === 'inbox/configure' || detail?.hint))) void loadPolicy()
    })
    return () => { alive = false; generation += 1; off(); stop() }
  })

  const tabClass =
    'flex h-7 flex-1 items-center justify-center gap-1.5 rounded-lg px-2 text-[12.5px] text-muted outline-none transition-colors duration-150 hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=active]:bg-surface data-[state=active]:font-medium data-[state=active]:text-ink'
</script>

{#snippet sender(account: string)}
  <bdi dir="ltr" class="font-mono text-[12.5px]">{shortId(account)}</bdi>
{/snippet}

<div class="@container flex h-full min-h-0" data-testid="inbox">
  {#if error === 'LOCKED'}
    <Locked {status} testid="inbox-locked" />
  {:else}
    <section class="flex w-[280px] shrink-0 flex-col border-e border-line @min-[900px]:w-[320px]">
      <div class="grid gap-2 p-3 pb-2">
        <div class="flex items-center gap-1.5">
          {#if unlocked}
            <button class={buttonClass('primary', 'md', 'flex-1')} onclick={() => (writing = true)} data-testid="letter-compose">
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
      </div>
      {#if error}<Notice failure={error} tone="danger" testid="inbox-error">{error}</Notice>{/if}
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
              onclick={() => (selected = item.order_id)}
            >
              <Avatar id={account} />
              <span class="min-w-0">
                <span class={cn('flex items-center gap-1.5 truncate text-[13px] leading-snug', box !== 'sent' && !item.read ? 'font-semibold' : 'font-medium')}>
                  {#if box === 'sent'}<span class="text-faint">{t('to')}</span>{/if}
                  {@render sender(account)}
                </span>
                <span class="block truncate text-[12px] leading-snug text-faint">
                  {#if box === 'sent'}
                    {item.paid ? t('paidLetter') : t('freeLetter')}
                  {:else if item.state === 'visible'}
                    {item.text.split('\n')[0]}
                  {:else}
                    {t('aborted')}
                  {/if}
                </span>
              </span>
              <span class="flex flex-col items-end gap-1 pt-0.5 text-[11px] text-faint">
                {#if box !== 'sent'}<span class="whitespace-nowrap">{ago(item.created_at)}</span>{/if}
                {#if box !== 'sent' && !item.read}<span class="size-2 rounded-full bg-forest" title={t('unread')}></span>{/if}
                {#if box === 'sent' && item.paid}<Icon name="coins" class="size-3.5 text-gem" />{/if}
              </span>
            </button>
          </li>
        {:else}
          <li class="grid justify-items-center gap-2 px-4 py-10 text-center">
            <span class="grid size-11 place-items-center text-faint [&_svg]:size-6"><Icon name="inbox" /></span>
            <p class="text-[13px] text-muted">{!loaded || (refreshing && box !== 'sent') ? t('loading') : box === 'sent' ? t('noSent') : t('noLetters')}</p>
          </li>
        {/each}
      </ul>
    </section>

    <section class="min-w-0 flex-1 overflow-y-auto">
      {#if letter}
        <article class="mx-auto max-w-2xl px-8 py-6" data-testid="letter">
          <header class="flex items-start gap-3">
            <Avatar id={letter.sender} size="lg" />
            <div class="min-w-0 flex-1">
              <button class="max-w-full truncate font-mono text-[13px] font-medium outline-none hover:text-forest focus-visible:underline" title={t('copy')} onclick={() => copyText(letter.sender)}>
                <bdi dir="ltr">{letter.sender}</bdi>
              </button>
              <p class="text-[12px] text-faint">{when(letter.created_at)}</p>
            </div>
            {#if unlocked}
              <div class="flex shrink-0 gap-1">
                {#if !letter.read}
                  <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => mark(letter, true, letter.archived)} data-testid="letter-read">
                    <Icon name="check" />{t('markRead')}
                  </button>
                {/if}
                <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => mark(letter, true, !letter.archived)} data-testid="letter-archive">
                  <Icon name={letter.archived ? 'unarchive' : 'archive'} />{letter.archived ? t('moveToInbox') : t('archive')}
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
        <article class="mx-auto grid max-w-2xl gap-4 px-8 py-6" data-testid="sent-letter">
          <header class="flex items-start gap-3">
            <Avatar id={order.recipient} size="lg" />
            <div class="min-w-0 flex-1">
              <p class="text-[12px] text-faint">{t('to')}</p>
              <bdi dir="ltr" class="block truncate font-mono text-[13px] font-medium">{order.recipient}</bdi>
            </div>
            <span class={cn('rounded-md px-2 py-0.5 text-[11.5px] font-medium', order.paid ? 'bg-gem-pale text-gem' : 'bg-wash text-muted')}>
              {order.paid ? t('paidLetter') : t('freeLetter')}
            </span>
          </header>
          <dl class="grid gap-1">
            <div class="grid grid-cols-[120px_1fr] items-center gap-4 py-2.5">
              <dt class="text-[12px] text-faint">{t('order')}</dt>
              <dd class="truncate font-mono text-[12px]" title={order.order_id}>{order.order_id}</dd>
            </div>
            <div class="grid grid-cols-[120px_1fr] items-center gap-4 py-2.5">
              <dt class="text-[12px] text-faint">{t('status')}</dt>
              <dd class="flex items-center justify-between gap-2">
                <span data-testid="order-state">{orders[order.order_id]?.state ?? '-'}</span>
                <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={() => check(order)}><Icon name="sync-now" />{t('checkStatus')}</button>
              </dd>
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
                        <span class="rounded bg-pale px-1.5 text-[11px] font-medium text-forest">{leg.status}</span>
                        {leg.kind} · {leg.amount} → <bdi dir="ltr" class="font-mono">{shortId(leg.to)}</bdi>
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
  <Compose bind:open={writing} {mod} />
  <Rules bind:open={ruling} {policy} onSaved={(next) => (policy = next)} />
{/key}
