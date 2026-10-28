<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import TextField from '../../components/TextField.svelte'
  import Id from '../../components/page/Id.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, errorText, onChange, size, when } from '../../rpc'

  let { unlocked }: { unlocked: boolean } = $props()
  let links = $state<any[]>([])
  let loaded = $state(false)
  let busy = $state(false)
  let error = $state('')
  let query = $state('')
  let confirmation = $state<any | null>(null)
  let limit = $state(300)
  let revision = 0
  let alive = true
  const current = (at: number) => alive && unlocked && at === revision
  const states: Record<string, Label> = { active: 'linkStateActive', revoked: 'linkStateRevoked', expired: 'linkStateExpired', exhausted: 'linkStateExhausted' }
  const rows = $derived(links.filter(link => `${link.name ?? ''} ${link.share_id}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())))
  const shown = $derived(rows.slice(0, limit))

  $effect(() => { query; limit = 300 })
  $effect(() => {
    if (!unlocked) { revision++; links = []; loaded = false; busy = false; error = ''; confirmation = null; query = '' }
    else void untrack(refresh)
  })

  async function refresh() {
    if (busy || !unlocked) return
    busy = true; error = ''
    const at = ++revision
    try {
      if (!loaded) {
        const saved = await call<any[]>('files/share_links').catch(() => null)
        if (!current(at)) return
        if (saved) { links = saved; loaded = true }
      }
      const next = await call<any[]>('files/share_links', { refresh: true })
      if (current(at)) { links = next; loaded = true }
    } catch (e) { if (current(at)) error = errorText(e) }
    finally { if (current(at)) busy = false }
  }

  /** Library changes, background backups included, re-read the saved links; only opening or refreshing the page reads the relay. */
  async function reload() {
    if (busy || !unlocked) return
    const at = ++revision
    const saved = await call<any[]>('files/share_links').catch(() => null)
    if (saved && current(at)) { links = saved; loaded = true }
  }

  async function revoke() {
    if (!confirmation || busy || !unlocked) return
    busy = true; error = ''
    const at = ++revision
    const shareId = confirmation.share_id
    try {
      const next = await call<any>('files/share_revoke', { share_id: shareId })
      if (current(at)) { links = links.map(link => link.share_id === shareId ? next : link); confirmation = null }
    } catch (e) { if (current(at)) error = errorText(e) }
    finally { if (current(at)) busy = false }
  }

  onMount(() => {
    const off = onChange(['files/changed'], () => void reload())
    return () => { alive = false; revision++; off() }
  })
</script>

<section class="flex h-full min-w-0 flex-col" data-testid="files-links-page">
  <header class="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
    <div class="min-w-0 flex-1"><h2 class="text-[15px] font-semibold">{t('shareLinks')}</h2><p class="mt-0.5 text-[12px] text-faint">{t('linksSubtitle')}</p></div>
    <button class={buttonClass('secondary')} disabled={busy || !unlocked} onclick={refresh} data-testid="links-refresh"><Icon name="sync" />{t('refreshLinks')}</button>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto p-5">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    <TextField label={t('linksSearch')} bind:value={query} type="search" disabled={!unlocked} data-testid="links-search" />
    {#if loaded}
      <ul class="mt-3 divide-y divide-line" data-testid="links-list">
        {#each shown as link (link.share_id)}
          <li class="flex min-w-0 flex-wrap items-center gap-3 py-3" data-share={link.share_id}>
            <Icon name="link" class="shrink-0 text-faint" />
            <div class="min-w-0 basis-48 flex-1">
              {#if link.name}<p class="break-words text-[13px] font-medium"><bdi dir="auto">{link.name}</bdi></p>{:else}<Id id={link.share_id} />{/if}
              <p class="mt-0.5 text-[11.5px] text-faint" data-testid="link-state">{t(states[link.state] ?? 'linkStateActive')} · {t('linkDownloads', { n: link.downloads ?? 0 })}{#if link.size != null} · {size(link.size)}{/if}</p>
              <p class="mt-0.5 text-[11.5px] text-faint">{t('linkExpires', { time: when(link.expires_at) })}</p>
            </div>
            <div class="flex flex-wrap gap-1">
              {#if link.url && link.state === 'active'}<button class={buttonClass('ghost', 'xs')} onclick={() => copyText(link.url)} data-testid="links-copy">{t('linkCopy')}</button>{/if}
              {#if link.state !== 'revoked'}<button class={buttonClass('ghost', 'xs', 'text-danger')} disabled={busy || !unlocked} onclick={() => { error = ''; confirmation = link }} data-testid="links-revoke">{t('linkRevoke')}</button>{/if}
            </div>
          </li>
        {/each}
      </ul>
      {#if shown.length < rows.length}<button class={buttonClass('secondary', 'sm', 'mt-3')} onclick={() => limit += 300}>{t('moreRows', { n: rows.length - shown.length })}</button>{/if}
      {#if !rows.length}<p class="py-12 text-center text-[13px] text-faint" data-testid="links-empty">{t('linksEmpty')}</p>{/if}
    {:else if busy}<p class="py-12 text-center text-[13px] text-faint">{t('loading')}</p>{/if}
  </div>
</section>

<Dialog bind:open={() => !!confirmation, next => !next && (confirmation = null)} title={t('linkRevoke')} icon="link" tone="danger" testid="links-confirm">
  <p class="break-words text-[13px] font-medium"><bdi dir="auto">{confirmation?.name ?? confirmation?.share_id}</bdi></p>
  <p class="mt-2 text-[13px] text-muted">{t('linkRevokeHint')}</p>
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#snippet footer()}<button class={buttonClass('ghost')} onclick={() => confirmation = null}>{t('cancel')}</button><button class={buttonClass('danger')} disabled={busy || !unlocked} onclick={revoke} data-testid="links-confirm-revoke">{t('linkRevoke')}</button>{/snippet}
</Dialog>
