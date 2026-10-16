<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { copyText } from '../../components/clipboard'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import TagInput from '../../components/TagInput.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, call, code, size, when } from '../../rpc'
  import { fileIcon, originLabel, previewable, statusIcon, statusLabel, statusOf, statusTone, type Status } from './files'

  /**
   * One file, opened: a preview, its tags and note, where each version
   * stands (on this device, in the cloud, both) and what can be done with
   * it. Backups, sharing and deleting are explicit actions here; nothing
   * about the file leaves the device otherwise.
   */
  let {
    file,
    versions,
    trash,
    unlocked,
    reload,
    onOpen,
    onExport,
    onBackup,
    onShare,
    links,
    onLink,
    onRevokeLink,
    onRefreshLinks,
    onMove,
    onRename,
    onDiscard,
    onDeleteCloud
  }: {
    file: any
    versions: any[]
    trash: boolean
    unlocked: boolean
    reload: () => Promise<void>
    onOpen: (version: string | null) => void
    onExport: (version: string | null) => void
    onBackup: (allVersions: boolean) => void
    onShare: (version: string | null) => void
    links: any[]
    onLink: (version: string | null) => void
    onRevokeLink: (shareId: string) => void
    onRefreshLinks: () => void
    onMove: () => void
    onRename: () => void
    onDiscard: () => void
    onDeleteCloud: (uploads: string[]) => void
  } = $props()

  let busy = $state(false)
  let preview = $state<{ kind: string; mime: string; url?: string; text?: string; thumbnail: boolean } | null>(null)
  let previewFor = $state('')
  let note = $state('')
  let editingNote = $state(false)
  let checked = $state('')

  const status = $derived<Status>(statusOf(file.status))
  const current = $derived(versions.find((v) => v.current) ?? versions.at(-1))
  const canRead = $derived(unlocked && !trash && status !== 'cloud_only')

  async function attempt(work: () => Promise<unknown>, done?: string) {
    busy = true
    try {
      await work()
      if (done) toast(() => done)
    } catch (error) {
      toast(() => (code(error) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(error)}`))
    } finally {
      busy = false
    }
  }

  // The preview follows the current version; a failure leaves none.
  $effect(() => {
    const key = `${file.id}:${file.current}:${status}`
    if (previewFor === key) return
    previewFor = key
    preview = null
    if (!canRead || !previewable(file.mime)) return
    void call('files/preview', { id: file.id })
      .then((p: any) => {
        if (previewFor !== key) return
        if (p.kind === 'image' && p.content_base64url) preview = { kind: 'image', mime: p.mime, url: `data:${p.mime};base64,${toBase64(p.content_base64url)}`, thumbnail: !!p.thumbnail }
        else if (p.kind === 'text' && p.content_base64url) preview = { kind: 'text', mime: p.mime, text: decodeText(p.content_base64url), thumbnail: false }
      })
      .catch(() => (preview = null))
  })

  $effect(() => {
    if (!editingNote) note = file.note ?? ''
  })

  /** base64url to the base64 a data URL wants. */
  const toBase64 = (text: string) => text.replace(/-/g, '+').replace(/_/g, '/') + '='.repeat((4 - (text.length % 4)) % 4)
  function decodeText(text: string): string {
    const bytes = Uint8Array.from(atob(toBase64(text)), (c) => c.charCodeAt(0))
    return new TextDecoder().decode(bytes)
  }

  const star = () => attempt(() => call('files/update', { id: file.id, starred: !file.starred }).then(reload))
  const tags = (next: string[]) => attempt(() => call('files/update', { id: file.id, tags: next }).then(reload))
  const saveNote = () =>
    attempt(async () => {
      await call('files/update', { id: file.id, note })
      editingNote = false
      await reload()
    }, t('saved'))
  const verify = () =>
    attempt(async () => {
      checked = t('verifying')
      const report: any = await call('files/verify', { id: file.id })
      const bad = report.versions.filter((v: any) => !v.ok)
      checked = bad.length ? `${t('error')}: ${bad[0].error}` : `${t('verified')} · ${size(file.size)} · sha256 ${String(file.sha256).slice(0, 16)}…`
    })
  const useVersion = (version: string) => attempt(() => call('files/version_use', { id: file.id, version }).then(reload))
  const deleteVersion = (version: string) => attempt(() => call('files/version_delete', { id: file.id, version }).then(reload))
  const evict = (version: string) => attempt(() => call('files/evict', { id: file.id, version }).then(reload))

  const tones: Record<string, string> = { neutral: 'text-faint', forest: 'text-forest', gem: 'text-gem', warn: 'text-warn', danger: 'text-danger', ok: 'text-ok' }
  const dateFormat = $derived(new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short' }))
</script>

{#snippet row(label: string)}
  <dt class="pt-2 text-[12px] text-faint @min-[440px]:pt-0">{label}</dt>
{/snippet}

<section class="@container flex h-full min-w-0 flex-col" aria-label={file.name} data-testid="files-detail">
  <header class="flex items-start gap-3 px-6 pt-5 pb-4">
    <span class="grid size-11 shrink-0 place-items-center rounded-xl border border-line bg-surface text-forest shadow-xs [&_svg]:size-5">
      <Icon name={fileIcon(file.mime)} />
    </span>
    <div class="min-w-0 flex-1">
      <h2 class="text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]" data-testid="files-name">{file.name}</h2>
      <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[12px] text-faint">
        <span>{size(file.size)}</span>
        <span>·</span>
        <span class="truncate">/{[...(file.path ?? [])].join('/')}</span>
        <Pill tone={statusTone[status]} testid="files-status"><Icon name={statusIcon[status]} />{t(statusLabel[status])}</Pill>
      </div>
    </div>
    {#if unlocked}
      <div class="flex shrink-0 items-center gap-1">
        {#if trash}
          <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={onDiscard} data-testid="files-restore">
            <Icon name="restore" />{t('restore')}
          </button>
        {:else}
          <Tooltip content={file.starred ? t('unstar') : t('star')}>
            {#snippet children(props)}
              <button {...props} class={buttonClass('ghost', 'md', cn('size-8 px-0', file.starred && 'text-gem hover:not-disabled:text-gem'))} aria-label={file.starred ? t('unstar') : t('star')} aria-pressed={file.starred} disabled={busy} onclick={star} data-testid="files-star">
                <Icon name={file.starred ? 'starred' : 'star'} />
              </button>
            {/snippet}
          </Tooltip>
          <Tooltip content={t('openFileHint')}>
            {#snippet children(props)}
              <button {...props} class={buttonClass('secondary', 'md')} disabled={busy || !canRead} onclick={() => onOpen(null)} data-testid="files-open">
                <Icon name="external" />{t('openFile')}
              </button>
            {/snippet}
          </Tooltip>
          <button class={buttonClass(status === 'local' || status === 'failed' ? 'primary' : 'secondary', 'md')} disabled={busy || status === 'backed_up' || status === 'backing_up' || status === 'cloud_only' || status === 'too_large'} onclick={() => onBackup(false)} data-testid="files-backup">
            <Icon name="cloud-up" />{t('backUp')}
          </button>
          <DropdownMenu.Root dir={localeDir(locale())}>
            <Tooltip content={t('moreActions')}>
              {#snippet children(props)}
                <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('moreActions')} data-testid="files-more">
                  <Icon name="more" />
                </DropdownMenu.Trigger>
              {/snippet}
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-56')} data-testid="files-more-menu">
                <DropdownMenu.Item class={menuItemClass} disabled={!canRead} onSelect={() => onExport(null)}>
                  <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('exportFile')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={!canRead || status === 'too_large'} onSelect={() => onShare(null)}>
                  <span class="flex items-center gap-2"><Icon name="send" class="text-faint rtl:-scale-x-100" />{t('shareToChannel')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={!canRead || status === 'too_large'} onSelect={() => onLink(null)} data-testid="files-link">
                  <span class="flex items-center gap-2"><Icon name="link" class="text-faint" />{t('shareByLink')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={versions.length > 1 && status === 'backed_up'} onSelect={() => onBackup(true)}>
                  <span class="flex items-center gap-2"><Icon name="cloud-up" class="text-faint" />{t('backUpAll')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={!canRead} onSelect={verify}>
                  <span class="flex items-center gap-2"><Icon name="verified" class="text-faint" />{t('verifyFile')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Separator class="my-1 h-px bg-line" />
                <DropdownMenu.Item class={menuItemClass} onSelect={onRename}>
                  <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('renameFile')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} onSelect={onMove}>
                  <span class="flex items-center gap-2"><Icon name="folder" class="text-faint" />{t('moveTo')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Separator class="my-1 h-px bg-line" />
                <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={onDiscard} data-testid="files-trash">
                  <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
    {/if}
  </header>

  {#if trash}
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-xl border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
      <Icon name="trash" class="shrink-0" /><span>{t('inTrash')}</span>
    </div>
  {:else if status === 'cloud_only'}
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-xl border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-muted" data-testid="files-cloud-only">
      <Icon name="cloud-down" class="shrink-0 text-forest" /><span>{t('statusCloudOnly')} · {t('releaseLocalHint')}</span>
    </div>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
    {#if preview}
      <div class="mb-3 overflow-hidden rounded-xl border border-line bg-surface-2 shadow-xs" data-testid="files-preview">
        {#if preview.kind === 'image'}
          <img src={preview.url} alt={file.name} class="mx-auto max-h-72 object-contain" />
          {#if preview.thumbnail}<p class="px-3 py-1.5 text-[11px] text-faint">{t('thumbnail')}</p>{/if}
        {:else}
          <pre class="max-h-72 overflow-auto px-4 py-3 font-mono text-[12px] leading-relaxed whitespace-pre-wrap select-text">{preview.text}</pre>
        {/if}
      </div>
    {/if}

    <dl class="divide-y divide-line rounded-xl border border-line bg-surface px-4 shadow-xs" data-testid="files-facts">
      <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
        {@render row(t('fileType'))}
        <dd class="min-w-0 truncate select-text">{file.mime}</dd>
      </div>
      <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
        {@render row(t('modified'))}
        <dd class="min-w-0" title={when(file.updated_at)}>{dateFormat.format(file.updated_at)}</dd>
      </div>
      <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
        {@render row('SHA-256')}
        <dd class="min-w-0 truncate font-mono text-[12px] select-text" dir="ltr">{file.sha256}</dd>
      </div>
      <div class="grid gap-x-4 py-2 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-start">
        {@render row(t('tags'))}
        <dd class="min-w-0">
          {#if unlocked && !trash}
            <TagInput bind:tags={() => file.tags ?? [], (next) => void tags(next)} placeholder={t('tagsAdd')} testid="files-tags" />
          {:else}
            <div class="flex flex-wrap gap-1.5">
              {#each file.tags ?? [] as tag, i (i)}<span class="rounded-md bg-wash px-1.5 py-px text-[11.5px] text-ink-2">#{tag}</span>{/each}
            </div>
          {/if}
        </dd>
      </div>
      <div class="grid gap-x-4 py-2 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-start">
        {@render row(t('noteLabel'))}
        <dd class="min-w-0">
          {#if editingNote}
            <textarea bind:value={note} rows="3" class="text-[13px] text-ink" placeholder={t('notePlaceholder')} data-testid="files-note"></textarea>
            <div class="mt-1.5 flex justify-end gap-2">
              <button class={buttonClass('ghost', 'sm')} onclick={() => (editingNote = false)}>{t('cancel')}</button>
              <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={saveNote} data-testid="files-note-save">{t('save')}</button>
            </div>
          {:else}
            <button class="w-full rounded-lg px-1 py-0.5 text-start text-[13px] leading-relaxed whitespace-pre-wrap hover:bg-wash disabled:hover:bg-transparent" disabled={!unlocked || trash} onclick={() => (editingNote = true)}>
              {file.note || (unlocked && !trash ? t('notePlaceholder') : '—')}
            </button>
          {/if}
        </dd>
      </div>
      {#if checked}
        <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
          {@render row(t('verifyFile'))}
          <dd class={cn('flex items-center gap-1 text-[12.5px]', checked.startsWith(t('error')) ? 'text-danger' : 'text-ok')} data-testid="files-checked"><Icon name="verified" class="size-3.5" />{checked}</dd>
        </div>
      {/if}
    </dl>

    {#if links.length}
      <div class="mt-5 mb-2 flex items-center justify-between">
        <h3 class="text-[12px] font-medium text-faint">{t('shareLinks')} · {links.length}</h3>
        <button class="text-[11.5px] text-forest hover:underline" disabled={busy || !unlocked} onclick={onRefreshLinks}>{t('refreshLinks')}</button>
      </div>
      <ul class="grid gap-1.5" data-testid="files-links">
        {#each links as link (link.share_id)}
          <li class="flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5 text-[12px]">
            <Icon name="link" class="shrink-0 text-faint" />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{link.state === 'active' ? t('linkStateActive') : link.state === 'revoked' ? t('linkStateRevoked') : t('linkStateExpired')} · {t('linkDownloads', { n: link.downloads })}{link.max_downloads ? ` / ${link.max_downloads}` : ''}{link.password ? ` · ${t('linkPassword')}` : ''}</span>
              <span class="block truncate text-[11px] text-faint">{t('linkExpires', { time: new Date(link.expires_at).toLocaleString() })}</span>
            </span>
            {#if link.url && link.state === 'active'}
              <button class={buttonClass('ghost', 'xs')} onclick={() => copyText(link.url)} aria-label={t('linkCopy')}><Icon name="copy" /></button>
            {/if}
            {#if link.state === 'active'}
              <button class={buttonClass('ghost', 'xs', 'text-danger')} disabled={busy || !unlocked} onclick={() => onRevokeLink(link.share_id)} data-testid="files-link-revoke">{t('linkRevoke')}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <h3 class="mt-5 mb-2 text-[12px] font-medium text-faint">{t('versions')} · {versions.length}</h3>
    <ol class="divide-y divide-line rounded-xl border border-line bg-surface shadow-xs" data-testid="files-versions">
      {#each [...versions].reverse() as version (version.id)}
        {@const vs = statusOf(version.status)}
        <li class="flex items-center gap-3 px-4 py-2.5">
          <span class={cn('grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface-2', tones[statusTone[vs]])}>
            <Icon name={statusIcon[vs]} />
          </span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-1.5 text-[13px]">
              <span title={when(version.created_at)}>{dateFormat.format(version.created_at)}</span>
              {#if version.current}<Pill tone="forest">{t('current')}</Pill>{/if}
            </div>
            <div class="text-[11.5px] text-faint">
              {size(version.size)} · {t(originLabel[version.origin?.type] ?? 'originImported')} · {t('fromDevice', { device: String(version.device ?? '').slice(0, 8) })} · {t(statusLabel[vs])}
              {#if version.backup}
                · {t('statusBackedUp')} {ago(version.backup.backed_up_at)}
              {/if}
            </div>
          </div>
          {#if unlocked && !trash}
            <DropdownMenu.Root dir={localeDir(locale())}>
              <DropdownMenu.Trigger class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('moreActions')} data-testid="files-version-more">
                <Icon name="more" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Portal>
                <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-56')}>
                  {#if !version.current}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => useVersion(version.id)}>
                      <span class="flex items-center gap-2"><Icon name="check" class="text-faint" />{t('useVersion')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Item class={menuItemClass} disabled={!version.local} onSelect={() => onOpen(version.id)}>
                    <span class="flex items-center gap-2"><Icon name="external" class="text-faint" />{t('openFile')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Item class={menuItemClass} disabled={!version.local} onSelect={() => onExport(version.id)}>
                    <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('exportFile')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Item class={menuItemClass} disabled={!version.local || version.size > 100 * 1024 * 1024} onSelect={() => onShare(version.id)}>
                    <span class="flex items-center gap-2"><Icon name="send" class="text-faint rtl:-scale-x-100" />{t('shareToChannel')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                  {#if version.backup?.verified && version.local}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => evict(version.id)}>
                      <span class="flex items-center gap-2"><Icon name="cloud-down" class="text-faint" />{t('releaseLocal')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if version.backup}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => onDeleteCloud([version.backup.upload_id])}>
                      <span class="flex items-center gap-2"><Icon name="cloud-off" />{t('deleteCloudCopy')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if versions.length > 1}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => deleteVersion(version.id)}>
                      <span class="flex items-center gap-2"><Icon name="trash" />{t('deleteVersion')}</span>
                    </DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          {/if}
        </li>
      {/each}
    </ol>
  </div>

  <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-6 py-2.5 text-[11.5px] text-faint">
    <span class="inline-flex items-center gap-1.5"><Icon name="drive" class="size-3.5" />{t('createdAt')} {ago(file.created_at)}</span>
    <span title={file.id}>ID <bdi dir="ltr" class="font-mono">{String(file.id).slice(0, 10)}</bdi></span>
    {#if current?.backup}
      <span class="inline-flex items-center gap-1.5 text-ok"><Icon name="sync" class="size-3.5" />{t('backupGeneration')} {current.backup.generation}</span>
    {/if}
  </footer>
</section>
