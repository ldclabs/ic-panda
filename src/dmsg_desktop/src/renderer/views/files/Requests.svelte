<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import TextField from '../../components/TextField.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText, size, when } from '../../rpc'
  import RequestComposer from './RequestComposer.svelte'
  import RequestDetail from './RequestDetail.svelte'
  import { isActiveRequest, requestLimit, requestStates } from './requests'

  /** Links that take files from people without an account. */
  let {
    unlocked,
    folders,
    onShowFile,
    onPlans
  }: {
    unlocked: boolean
    folders: any[]
    onShowFile: (id: string) => void
    onPlans: () => void
  } = $props()

  let links = $state<any[]>([])
  let usage = $state<any | null>(null)
  let loaded = $state(false)
  let busy = $state(false)
  let error = $state('')
  let query = $state('')
  let limit = $state(300)
  let composing = $state(false)
  let upgrading = $state(false)
  let opened = $state<string | null>(null)
  let confirmation = $state<any | null>(null)
  let revision = 0
  let alive = true
  const current = (at: number) => alive && unlocked && at === revision

  const active = $derived(links.filter(isActiveRequest).length)
  const allowed = $derived<number | null>(usage?.file_request_limit ?? null)
  const openedLink = $derived(links.find((link) => link.request_id === opened) ?? null)
  const rows = $derived(
    links.filter((link) =>
      `${link.title ?? ''} ${link.label ?? ''} ${link.request_id}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase())
    )
  )
  const shown = $derived(rows.slice(0, limit))

  $effect(() => {
    query
    limit = 300
  })
  $effect(() => {
    if (!unlocked) {
      revision++
      links = []
      usage = null
      loaded = false
      busy = false
      error = ''
      confirmation = null
      composing = false
      upgrading = false
      opened = null
      query = ''
    } else void untrack(refresh)
  })

  async function refresh() {
    if (busy || !unlocked) return
    busy = true
    error = ''
    const at = ++revision
    try {
      if (!loaded) {
        const saved = await call<any[]>('files/requests').catch(() => null)
        if (!current(at)) return
        if (saved) {
          links = saved
          loaded = true
        }
      }
      const [next, quota] = await Promise.all([
        call<any[]>('files/requests', { refresh: true }),
        call<any>('files/cloud_usage').catch(() => null)
      ])
      if (current(at)) {
        links = next
        usage = quota
        loaded = true
      }
    } catch (e) {
      if (current(at)) error = errorText(e)
    } finally {
      if (current(at)) busy = false
    }
  }

  async function reload() {
    if (busy || !unlocked) return
    const at = ++revision
    const saved = await call<any[]>('files/requests').catch(() => null)
    if (saved && current(at)) {
      links = saved
      loaded = true
    }
  }

  function create() {
    error = ''
    if (allowed === 0) upgrading = true
    else composing = true
  }

  async function close() {
    if (!confirmation || busy || !unlocked) return
    busy = true
    error = ''
    const at = ++revision
    const id = confirmation.request_id
    try {
      const next = await call<any>('files/request_revoke', { request_id: id })
      if (current(at)) {
        links = links.map((link) => (link.request_id === id ? next : link))
        confirmation = null
      }
    } catch (e) {
      if (current(at)) error = errorText(e)
    } finally {
      if (current(at)) busy = false
    }
  }

  onMount(() => {
    // A new arrival reads the relay; anything else re-reads what is saved here.
    const off = window.dmsg.onEvent((event) => {
      const method = (event.data as any)?.method
      if (event.type === 'files/changed' && method === 'file-request') {
        if (!busy) void refresh()
      } else if (event.type === 'files/changed' || event.type === 'resync') void reload()
    })
    return () => {
      alive = false
      revision++
      off()
    }
  })
</script>

{#if openedLink}
  {#key openedLink.request_id}
    <RequestDetail
      link={openedLink}
      {unlocked}
      {folders}
      {onShowFile}
      onBack={() => {
        opened = null
        void refresh()
      }}
      onChanged={refresh}
    />
  {/key}
{:else}
  <section class="flex h-full min-w-0 flex-col" data-testid="files-requests-page">
    <header class="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
      <div class="min-w-0 flex-1">
        <h2 class="text-[15px] font-semibold">{t('fileRequests')}</h2>
        <p class="mt-0.5 text-[12px] text-faint">{t('requestsSubtitle')}</p>
      </div>
      <button class={buttonClass('secondary')} disabled={busy || !unlocked} onclick={refresh} data-testid="requests-refresh"
        ><Icon name="sync" />{t('refreshLinks')}</button
      >
      <button class={buttonClass('primary')} disabled={!unlocked} onclick={create} data-testid="requests-new"
        ><Icon name="add" />{t('requestNew')}</button
      >
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto p-5">
      {#if error}<Notice failure={error} tone="danger" testid="requests-error">{error}</Notice>{/if}
      {#if allowed}
        <p class="mb-3 text-[12px] text-faint" data-testid="requests-usage">
          {t('requestsUsage', {
            active,
            limit: allowed,
            size: usage?.backup_max_file_bytes ? requestLimit(usage.backup_max_file_bytes) : '-'
          })}
        </p>
      {/if}
      <TextField label={t('linksSearch')} bind:value={query} type="search" disabled={!unlocked} data-testid="requests-search" />
      {#if loaded}
        <ul class="mt-3 divide-y divide-line" data-testid="requests-list">
          {#each shown as link (link.request_id)}
            <li class="flex min-w-0 flex-wrap items-center gap-3 py-3" data-request={link.request_id} data-status={link.status}>
              <Icon name="inbox" class="shrink-0 text-faint" />
              <div class="min-w-0 basis-48 flex-1">
                <button
                  class="block w-full min-w-0 rounded text-start outline-none focus-visible:ring-3 focus-visible:ring-focus/35"
                  onclick={() => (opened = link.request_id)}
                  data-testid="requests-open"
                >
                  {#if link.title}
                    <span class="block break-words text-[13px] font-medium"><bdi dir="auto">{link.title}</bdi></span>
                  {:else}
                    <Id id={link.request_id} />
                  {/if}
                  {#if link.label}<span class="mt-0.5 block break-words text-[12px] text-muted"><bdi dir="auto">{link.label}</bdi></span>{/if}
                </button>
                <p class="mt-0.5 text-[11.5px] text-faint" data-testid="requests-state">
                  {t(requestStates[link.status] ?? 'requestStateOpen')} · {t('requestFilesCount', { n: link.files ?? 0, size: size(link.bytes ?? 0) })}
                  {#if link.unread}
                    · <span class="font-medium text-forest" data-testid="requests-unread">{t('requestUnread')}</span>
                  {/if}
                </p>
                <p class="mt-0.5 text-[11.5px] text-faint">{t('linkExpires', { time: when(link.expires_at) })}</p>
                {#if !link.url}<p class="mt-0.5 text-[11.5px] text-warn">{t('requestNoKey')}</p>{/if}
              </div>
              <div class="flex flex-wrap gap-1">
                {#if link.url && isActiveRequest(link)}
                  <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(link.url)} data-testid="requests-copy">{t('linkCopy')}</button>
                {/if}
                {#if isActiveRequest(link)}
                  <button
                    class={buttonClass('ghost', 'xs', 'text-danger')}
                    disabled={busy || !unlocked}
                    onclick={() => {
                      error = ''
                      confirmation = link
                    }}
                    data-testid="requests-close">{t('requestClose')}</button
                  >
                {/if}
              </div>
            </li>
          {/each}
        </ul>
        {#if shown.length < rows.length}<button class={buttonClass('secondary', 'sm', 'mt-3')} onclick={() => (limit += 300)}>{t('moreRows', { n: rows.length - shown.length })}</button>{/if}
        {#if !rows.length}<p class="py-12 text-center text-[13px] text-faint" data-testid="requests-empty">{query.trim() ? t('linksEmpty') : t('requestsEmpty')}</p>{/if}
      {:else if busy}<p class="py-12 text-center text-[13px] text-faint">{t('loading')}</p>{/if}
    </div>
  </section>
{/if}

{#if unlocked}
  <RequestComposer bind:open={composing} {usage} onCreated={refresh} />
{/if}

<Dialog bind:open={upgrading} title={t('requestUpgradeTitle')} icon="inbox" testid="requests-upgrade">
  <p class="text-[13px] text-muted">{t('requestUpgrade')}</p>
  {#snippet footer()}<span class="flex-1"></span><button class={buttonClass('ghost')} onclick={() => (upgrading = false)}>{t('cancel')}</button><button
      class={buttonClass('primary')}
      onclick={() => {
        upgrading = false
        onPlans()
      }}
      data-testid="requests-plans">{t('requestUpgradeAction')}</button
    >{/snippet}
</Dialog>

<Dialog bind:open={() => !!confirmation, (next) => !next && (confirmation = null)} title={t('requestClose')} icon="inbox" tone="danger" testid="requests-confirm">
  <p class="break-words text-[13px] font-medium"><bdi dir="auto">{confirmation?.title ?? confirmation?.request_id}</bdi></p>
  <p class="mt-2 text-[13px] text-muted">{t('requestCloseHint')}</p>
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#snippet footer()}<span class="flex-1"></span><button class={buttonClass('ghost')} onclick={() => (confirmation = null)}>{t('cancel')}</button><button class={buttonClass('danger')} disabled={busy || !unlocked} onclick={close} data-testid="requests-confirm-close">{t('requestClose')}</button>{/snippet}
</Dialog>
