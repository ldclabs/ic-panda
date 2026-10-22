<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { onMount } from 'svelte'
  import { copyText } from '../../components/clipboard'
  import { localeDir } from '../../../shared/locales'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import TagInput from '../../components/TagInput.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, bridge, call, errorText, onChange, size, when } from '../../rpc'
  import { fileIcon, originLabel, previewable, statusIcon, statusLabel, statusOf, statusTone, type Status } from './files'

  /**
   * One file, opened: a preview of any version, its tags and note, where
   * each version stands (on this device, in the cloud, both), the file
   * statements signed over its versions, and what can be done with it.
   * Backups, sharing, deleting and statements are explicit actions here;
   * nothing about the file leaves the device otherwise. Each change comes
   * back through the daemon's `files/changed`.
   */
  let {
    file,
    versions,
    trash,
    unlocked,
    device,
    onOpen,
    onExport,
    onBackup,
    onShare,
    links,
    copies = [],
    copiesKnown = false,
    onManageCopies = () => {},
    onLink,
    onRevokeLink,
    onRefreshLinks,
    onMove,
    onRename,
    onReveal,
    onDiscard,
    onDeleteCloud
  }: {
    file: any
    versions: any[]
    trash: boolean
    unlocked: boolean
    /** This device's ID, to name the versions made here. */
    device: string
    onOpen: (version: string | null) => void
    onExport: (version: string | null) => void
    onBackup: (allVersions: boolean) => void
    onShare: (version: string | null) => void
    links: any[]
    copies?: any[]
    copiesKnown?: boolean
    onManageCopies?: () => void
    onLink: (version: string | null) => void
    onRevokeLink: (shareId: string) => void
    onRefreshLinks: () => void
    onMove: () => void
    onRename: () => void
    onReveal: () => void
    onDiscard: () => void
    onDeleteCloud: (uploads: string[]) => void
  } = $props()

  let busy = $state(false)
  let preview = $state<{ kind: string; mime: string; url?: string; text?: string; thumbnail: boolean } | null>(null)
  let previewFor = $state('')
  /** A version other than the current one being previewed. */
  let previewing = $state<any | null>(null)
  let note = $state('')
  let editingNote = $state(false)
  let checked = $state('')
  /** A version about to be deleted or have its local chunks freed. */
  let confirming = $state<{ kind: 'delete' | 'evict'; version: any } | null>(null)
  /** A statement being written over a version. */
  let stating = $state<{ version: any; text: string } | null>(null)
  let statements = $state<any[]>([])

  const status = $derived<Status>(statusOf(file.status))
  const current = $derived(versions.find((v) => v.current) ?? versions.at(-1))
  const canRead = $derived(unlocked && !trash && status !== 'cloud_only')
  const digests = $derived(new Set(versions.map((v) => v.sha256)))
  const mine = $derived(statements.filter((s) => s.statement?.profile === 'file_statement' && digests.has(s.statement.content?.sha256)))

  async function attempt(work: () => Promise<unknown>, done?: string) {
    busy = true
    try {
      await work()
      if (done) toast(() => done)
    } catch (error) {
      toast(() => errorText(error))
    } finally {
      busy = false
    }
  }

  // The preview follows the current version, or the one asked for; a failure leaves none.
  $effect(() => {
    const version = previewing?.id ?? null
    const key = `${file.id}:${version ?? file.current}:${status}`
    if (previewFor === key) return
    previewFor = key
    preview = null
    if (!unlocked || trash || (!version && status === 'cloud_only') || !previewable(file.mime)) return
    void call('files/preview', { id: file.id, version })
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

  const loadStatements = async () => {
    statements = unlocked ? await call<any[]>('attest/history').catch(() => []) : []
  }

  const star = () => attempt(() => call('files/update', { id: file.id, starred: !file.starred }))
  const tags = (next: string[]) => attempt(() => call('files/update', { id: file.id, tags: next }))
  const saveNote = () =>
    attempt(async () => {
      await call('files/update', { id: file.id, note })
      editingNote = false
    }, t('saved'))
  const verify = () =>
    attempt(async () => {
      checked = t('verifying')
      const report: any = await call('files/verify', { id: file.id })
      const bad = report.versions.filter((v: any) => !v.ok)
      checked = bad.length ? `${t('error')}: ${bad[0].error}` : `${t('verified')} · ${size(file.size)} · sha256 ${String(file.sha256).slice(0, 16)}…`
    })
  const useVersion = (version: string) => attempt(() => call('files/version_use', { id: file.id, version }))
  const settleVersion = () =>
    attempt(async () => {
      if (!confirming) return
      const { kind, version } = confirming
      await call(kind === 'delete' ? 'files/version_delete' : 'files/evict', { id: file.id, version: version.id })
      if (previewing?.id === version.id) previewing = null
      confirming = null
    })
  /** A statement over a version, signed in the approval window: the daemon reads its digest from the library and checks it again on approval. */
  const signStatement = () =>
    attempt(async () => {
      if (!stating?.text.trim()) return
      const request: any = await call('attest/request', {
        statement: { profile: 'file_statement', text: stating.text.trim(), content_type: file.mime || null, subject: file.name },
        library: { file: file.id, version: stating.version.id }
      })
      stating = null
      await bridge(() => window.dmsg.openApproval(request.id))
    })
  const exportBundle = (execution: string) =>
    attempt(async () => {
      const saved = await bridge<{ path: string } | null>(() => window.dmsg.exportBundle(execution))
      if (saved) toast(() => `${t('bundleSaved')}: ${saved.path}`)
    })

  onMount(() => {
    void loadStatements()
    return onChange(['approval/changed'], loadStatements)
  })

  const tones: Record<string, string> = { neutral: 'text-faint', forest: 'text-forest', gem: 'text-gem', warn: 'text-warn', danger: 'text-danger', ok: 'text-ok' }
  const dateFormat = $derived(new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short' }))
  const deviceName = (id: string) => (id && device && id.toLowerCase() === device.toLowerCase() ? t('thisDevice') : t('fromDevice', { device: String(id ?? '').slice(0, 8) }))
</script>

{#snippet row(label: string)}
  <dt class="pt-2 text-[12px] text-faint @min-[440px]:pt-0">{label}</dt>
{/snippet}

<section class="@container flex h-full min-w-0 flex-col" aria-label={file.name} data-testid="files-detail">
  <header class="flex flex-wrap items-start gap-x-3 gap-y-2 px-6 pt-5 pb-4">
    <div class="flex min-w-[min(100%,15rem)] flex-1 items-start gap-3">
      <span class="grid size-11 shrink-0 place-items-center rounded-xl border border-line bg-surface text-forest shadow-xs [&_svg]:size-5">
        <Icon name={fileIcon(file.mime)} />
      </span>
      <div class="min-w-0 flex-1">
        <h2 class="text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]" data-testid="files-name">{file.name}</h2>
        <div class="mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-[12px] text-faint">
          <span class="whitespace-nowrap">{size(file.size)}</span>
          <span>·</span>
          {#if trash}
            <span class="truncate" dir="ltr">/{[...(file.path ?? [])].join('/')}</span>
          {:else}
            <button class="truncate hover:text-ink hover:underline" dir="ltr" title={t('revealInFolder')} onclick={onReveal} data-testid="files-reveal">/{[...(file.path ?? [])].join('/')}</button>
          {/if}
          <Pill tone={statusTone[status]} testid="files-status"><Icon name={statusIcon[status]} />{t(statusLabel[status])}</Pill>
        </div>
      </div>
    </div>
    {#if unlocked}
      <div class="ms-auto flex shrink-0 items-center gap-1">
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
                <DropdownMenu.Item class={menuItemClass} disabled={!current} onSelect={() => (stating = { version: current, text: '' })} data-testid="files-state">
                  <span class="flex items-center gap-2"><Icon name="signature" class="text-faint" />{t('fileStatement')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={!versions.some((v) => v.status === 'local')} onSelect={() => onBackup(true)}>
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
        {#if previewing}
          <div class="flex items-center gap-2 border-b border-line px-3 py-1.5 text-[11.5px] text-muted" data-testid="files-preview-version">
            <span class="min-w-0 flex-1 truncate">{t('previewOf', { time: dateFormat.format(previewing.created_at) })}</span>
            <button class="shrink-0 text-forest hover:underline" onclick={() => (previewing = null)}>{t('backToCurrent')}</button>
          </div>
        {/if}
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
        <dd class="flex min-w-0 items-center gap-1">
          <span class="min-w-0 flex-1 truncate font-mono text-[12px] select-text" dir="ltr">{file.sha256}</span>
          <button class={buttonClass('ghost', 'xs', 'size-6 shrink-0 px-0')} aria-label={t('copy')} onclick={() => copyText(file.sha256)} data-testid="files-copy-sha"><Icon name="copy" /></button>
        </dd>
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
            <!-- svelte-ignore a11y_autofocus -->
            <textarea bind:value={note} rows="3" class="text-[13px] text-ink" placeholder={t('notePlaceholder')} autofocus onkeydown={(event) => event.key === 'Escape' && (editingNote = false)} data-testid="files-note"></textarea>
            <div class="mt-1.5 flex justify-end gap-2">
              <button class={buttonClass('ghost', 'sm')} onclick={() => (editingNote = false)}>{t('cancel')}</button>
              <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={saveNote} data-testid="files-note-save">{t('save')}</button>
            </div>
          {:else}
            <button class={cn('w-full rounded-lg px-1 py-0.5 text-start text-[13px] leading-relaxed whitespace-pre-wrap hover:bg-wash disabled:hover:bg-transparent', !file.note && 'text-faint')} disabled={!unlocked || trash} onclick={() => (editingNote = true)}>
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

    <div class="mt-5 flex items-center justify-between">
      <h3 class="text-[12px] font-medium text-faint">{t('storageTitle')} · {copiesKnown ? copies.length : '—'}</h3>
      <button class="text-[11.5px] text-forest hover:underline" onclick={onManageCopies}>{t('storageManage')}</button>
    </div>
    {#if copies.length}<ul class="mt-2 grid gap-1" data-testid="file-cloud-copies">{#each copies as copy (copy.id)}<li class="text-[12px] text-muted">{copy.kind === 'share' ? t('shareLinks') : copy.owner_kind === 'channel' ? t('storageAttachments') : t('cloudBackups')} · {size(copy.bytes)} · {copy.channel_name ?? copy.resource.slice(0,12)+'…'}</li>{/each}</ul>{/if}
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
              <span class="block truncate text-[11px] text-faint">{t('linkExpires', { time: when(link.expires_at) })}</span>
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

    {#if mine.length}
      <h3 class="mt-5 mb-2 text-[12px] font-medium text-faint">{t('fileStatements')} · {mine.length}</h3>
      <ul class="grid gap-1.5" data-testid="files-statements">
        {#each mine as entry (entry.execution)}
          {@const version = versions.find((v) => v.sha256 === entry.statement.content.sha256)}
          <li class="flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5 text-[12px]">
            <Icon name="signature" class="shrink-0 text-faint" />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{entry.statement.content.text ?? entry.statement.profile}</span>
              <span class="block truncate text-[11px] text-faint">{when(entry.created_at)}{version ? ` · ${t('versionOf', { time: dateFormat.format(version.created_at) })}` : ''}</span>
            </span>
            <Pill tone={entry.stage === 'complete' ? 'ok' : entry.error ? 'danger' : 'neutral'}>{entry.stage}</Pill>
            {#if entry.stage === 'complete'}
              <button class={buttonClass('ghost', 'xs')} disabled={busy} onclick={() => exportBundle(entry.execution)}>{t('exportBundle')}</button>
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
              {#if previewing?.id === version.id}<Pill>{t('previewVersion')}</Pill>{/if}
            </div>
            <div class="text-[11.5px] text-faint">
              {size(version.size)} · {t(originLabel[version.origin?.type] ?? 'originImported')} · {deviceName(version.device)} · {t(statusLabel[vs])}
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
                    <DropdownMenu.Item class={menuItemClass} disabled={!version.local || !previewable(file.mime)} onSelect={() => (previewing = version)} data-testid="files-version-preview">
                      <span class="flex items-center gap-2"><Icon name="eye" class="text-faint" />{t('previewVersion')}</span>
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
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => (stating = { version, text: '' })}>
                    <span class="flex items-center gap-2"><Icon name="signature" class="text-faint" />{t('fileStatement')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                  {#if version.backup?.verified && version.local}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (confirming = { kind: 'evict', version })} data-testid="files-version-evict">
                      <span class="flex items-center gap-2"><Icon name="cloud-down" class="text-faint" />{t('releaseLocal')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if version.backup}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} disabled={!version.local} onSelect={() => onDeleteCloud([version.backup.upload_id])}>
                      <span class="flex items-center gap-2"><Icon name="cloud-off" />{t('deleteCloudCopy')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if versions.length > 1}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (confirming = { kind: 'delete', version })} data-testid="files-version-delete">
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

<Dialog
  bind:open={() => !!confirming, (next) => !next && (confirming = null)}
  title={confirming?.kind === 'evict' ? t('releaseLocalTitle') : t('deleteVersionTitle')}
  description={confirming ? dateFormat.format(confirming.version.created_at) : undefined}
  icon={confirming?.kind === 'evict' ? 'cloud-down' : 'trash'}
  tone={confirming?.kind === 'evict' ? undefined : 'danger'}
  testid="files-version-dialog"
>
  {#if confirming?.kind === 'evict'}
    <p class="text-[13.5px] leading-relaxed">{t('releaseLocalHint')}</p>
  {:else if confirming}
    {#if confirming.version.backup}
      <p class="text-[13.5px] leading-relaxed">{t('deleteVersionHint')}</p>
    {:else}
      <Notice tone="warn">{t('deleteVersionUnbacked')}</Notice>
    {/if}
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (confirming = null)}>{t('cancel')}</button>
    <button class={buttonClass(confirming?.kind === 'evict' ? 'primary' : 'danger')} disabled={busy} onclick={settleVersion} data-testid="files-version-confirm">
      {confirming?.kind === 'evict' ? t('releaseLocal') : t('deleteVersion')}
    </button>
  {/snippet}
</Dialog>

<Dialog bind:open={() => !!stating, (next) => !next && (stating = null)} title={t('fileStatement')} description={file.name} icon="signature" testid="files-statement-dialog">
  {#if stating}
    <div class="grid gap-3">
      <Notice tone="info">{t('fileStatementHint')}</Notice>
      <p class="truncate font-mono text-[11.5px] text-faint" dir="ltr">sha256 {stating.version.sha256}</p>
      <label class="grid gap-1 text-[12px] text-faint">
        {t('fileStatementText')}
        <textarea bind:value={stating.text} rows="3" maxlength="4096" class="text-[13.5px] text-ink" data-testid="files-statement-text"></textarea>
      </label>
    </div>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (stating = null)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !stating?.text.trim()} onclick={signStatement} data-testid="files-statement-sign"><Icon name="signature" />{t('fileStatement')}</button>
  {/snippet}
</Dialog>
