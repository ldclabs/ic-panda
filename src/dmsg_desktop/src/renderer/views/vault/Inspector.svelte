<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { onDestroy } from 'svelte'
  import { localeDir } from '../../../shared/locales'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Id from '../../components/page/Id.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, bridge, call, errorText, size, when } from '../../rpc'
  import { focus as fileFocus } from '../files/focus.svelte'
  import History from './History.svelte'
  import ItemForm from './ItemForm.svelte'
  import OtpField from './OtpField.svelte'
  import SecretField from './SecretField.svelte'
  import ValueField from './ValueField.svelte'
  import { fieldName, isManaged, issueLabel, sections, typeIcon, typeLabel, typeOf, type Field } from './vault'

  /**
   * One item, opened: its fields by section with their actions beside
   * them, notes, attached files and where the version stands; its history
   * on request. Edit turns it into the item form, whose save names the
   * version it replaces: if another device moved on, the daemon keeps
   * both as a conflict. dMsg's own keys are shown read-only. The daemon's
   * `vault/changed` reloads the vault after each change.
   */
  let {
    entry,
    conflict,
    trash,
    unlocked,
    mod,
    device,
    issues,
    editing = $bindable(false),
    dirty = $bindable(false),
    go,
    onCompare,
    onDuplicated
  }: {
    entry: any
    conflict: boolean
    trash: boolean
    unlocked: boolean
    mod: string
    device: string | null
    issues: Map<string, string[]> | undefined
    editing?: boolean
    dirty?: boolean
    go: (path: string) => void
    onCompare: () => void
    onDuplicated: (id: string) => void
  } = $props()

  let busy = $state(false)
  let history = $state(false)
  let base = $state('')
  let names = $state<Record<string, any>>({})
  let copyingReference = $state<string | null>(null)
  let alive = true
  onDestroy(() => { alive = false })

  const item = $derived(entry.item)
  const type = $derived(typeOf(item))
  const managed = $derived(isManaged(item))
  const shownFields = $derived((item.fields as Field[]).filter((f) => f.value))
  const editable = $derived(unlocked && !trash && !managed)

  async function copyFieldReference(field: Field) {
    if (copyingReference || !editable || conflict || type === 'wallet' || field.type === 'otp') return false
    const id = entry.id, revision = entry.revision
    copyingReference = field.id
    try {
      const result = await call<{ uri: string }>('secrets/ref', { item_id: id, field_id: field.id })
      // A late response must not replace the clipboard after navigation, a lock or a source update.
      if (alive && editable && !conflict && entry.id === id && entry.revision === revision) return await copyText(result.uri, () => t('fieldReferenceCopied'))
      return false
    } catch (error) {
      if (alive) toast(() => errorText(error))
      return false
    } finally {
      if (alive) copyingReference = null
    }
  }

  $effect(() => {
    if (!editing) dirty = false
  })

  // Attached files by name and size, each asked for once; one gone from the library says so.
  const asked = new Set<string>()
  $effect(() => {
    for (const id of item.files ?? [])
      if (!asked.has(id)) {
        asked.add(id)
        void call('files/show', { id })
          .then((details: any) => (names = { ...names, [id]: details.file }))
          .catch(() => (names = { ...names, [id]: null }))
      }
  })

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

  const star = () => attempt(() => call('vault/edit', { id: entry.id, base: entry.revision, changes: { favorite: !item.favorite } }))
  const discard = () => attempt(() => call(trash ? 'vault/restore' : 'vault/delete', { id: entry.id }), trash ? t('restored') : t('movedToTrash'))
  const duplicate = () =>
    attempt(async () => {
      const copy: any = await call('vault/duplicate', { id: entry.id, title: t('copyTitle', { title: item.title }) })
      onDuplicated(copy.id)
    }, t('duplicated'))

  /** The form keeps the version it started from: a version synced in meanwhile makes the save a conflict, not an overwrite. */
  function startEdit() {
    base = entry.revision
    history = false
    editing = true
  }

  const save = (changes: Record<string, unknown>) =>
    attempt(async () => {
      await call('vault/edit', { id: entry.id, base, changes })
      dirty = false
      editing = false
    }, t('saved'))

  async function open(url: string) {
    try {
      await bridge(() => window.dmsg.openUrl(url))
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  function showFile(id: string) {
    fileFocus.id = id
    go('/files')
  }

  function formatted(field: Field): string {
    if (field.type === 'date') {
      const d = new Date(`${field.value}T00:00:00`)
      return Number.isNaN(d.getTime()) ? field.value : d.toLocaleDateString(locale(), { dateStyle: 'medium' })
    }
    if (field.type === 'month') {
      const d = new Date(`${field.value}-01T00:00:00`)
      return Number.isNaN(d.getTime()) ? field.value : d.toLocaleDateString(locale(), { year: 'numeric', month: 'long' })
    }
    return field.value
  }
</script>

{#snippet fieldRow(field: Field)}
  {@const name = fieldName(field, t)}
  {@const reference = field.type !== 'otp' && !managed && type !== 'wallet' && !trash ? () => copyFieldReference(field) : undefined}
  {@const referenceDisabled = !unlocked || conflict || copyingReference !== null}
  {@const referenceHint = conflict ? t('conflictItem') : t('fieldReferenceHint')}
  <div class="grid gap-x-4 gap-y-1 py-2 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-start" data-testid="vault-field" data-field={field.id}>
    <dt class="flex flex-wrap items-center gap-1 text-[12px] text-faint @min-[440px]:pt-1.5">
      <span class="[overflow-wrap:anywhere]">{name}</span>
      {#each issues?.get(field.id) ?? [] as issue (issue)}
        <span class="rounded bg-warn-pale px-1 text-[10.5px] font-medium text-warn" data-testid="vault-issue">{t(issueLabel[issue] ?? 'warning')}</span>
      {/each}
    </dt>
    <dd class="flex min-w-0 items-start">
      {#if field.type === 'concealed'}
        {#key entry.revision}
          <SecretField id={entry.id} field={field.id} {name} context={item.title} unlocked={unlocked && !trash} {reference} {referenceDisabled} {referenceHint} />
        {/key}
      {:else if field.type === 'otp'}
        {#if unlocked && !trash}
          {#key entry.revision}<OtpField id={entry.id} field={field.id} {name} context={item.title} {unlocked} />{/key}
        {:else}<span class="py-1 font-mono text-faint">••• •••</span>{/if}
      {:else}
        <ValueField {field} {name} context={item.title} identity={`${entry.id}:${entry.revision}:${field.id}`} formatted={formatted(field)} {unlocked}
          {reference} {referenceDisabled} {referenceHint}
          openWebsite={field.type === 'url' && /^https?:\/\//i.test(field.value) ? () => open(field.value) : undefined} />
      {/if}
    </dd>
  </div>
{/snippet}

<section class="@container flex h-full min-w-0 flex-col" aria-label={item.title} data-testid="vault-inspector">
  <header class="flex flex-wrap items-start gap-3 px-6 pt-5 pb-4">
    <div class="flex min-w-[min(100%,12rem)] flex-1 items-start gap-3">
      <span class="grid size-9 shrink-0 place-items-center text-forest [&_svg]:size-5">
        <Icon name={typeIcon[type]} />
      </span>
      <div class="min-w-0 flex-1">
        <div class="flex min-w-0 items-start gap-1.5">
          <h2 class="min-w-0 text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]" data-testid="vault-item-title">{item.title}</h2>
          {#if item.favorite}<span class="pt-0.5 text-gem" role="img" aria-label={t('starred')} title={t('starred')}><Icon name="starred" class="size-4" /></span>{/if}
        </div>
        <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[12px] text-faint">
          <span>{t(typeLabel[type])}</span>
          {#if managed}<span class="rounded-md bg-pale px-1.5 py-px text-[11.5px] text-forest" data-testid="vault-managed">{t('managedByDmsg')}</span>{/if}
          {#each item.tags as tag, i (i)}
            <span class="rounded-md bg-wash px-1.5 py-px text-[11.5px] text-ink-2">#{tag}</span>
          {/each}
        </div>
      </div>
    </div>
    {#if unlocked && !editing}
      <div class="ms-auto flex shrink-0 items-center gap-1">
        {#if trash}
          <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={discard} data-testid="vault-restore">
            <Icon name="restore" />{t('restore')}
          </button>
        {:else if !managed}
          <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={startEdit} data-testid="vault-edit-open">
            <Icon name="edit" />{t('edit')}
          </button>
        {/if}
        {#if !trash}
          <DropdownMenu.Root dir={localeDir(locale())}>
            <Tooltip content={t('moreActions')}>
              {#snippet children(props)}
                <DropdownMenu.Trigger {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('moreActions')} data-testid="vault-more">
                  <Icon name="more" />
                </DropdownMenu.Trigger>
              {/snippet}
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-48')} data-testid="vault-more-menu">
                {#if !managed}
                  <DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={star} data-testid="vault-star">
                    <span class="flex items-center gap-2"><Icon name={item.favorite ? 'starred' : 'star'} class="text-faint" />{item.favorite ? t('unstar') : t('star')}</span>
                  </DropdownMenu.Item>
                {/if}
                <DropdownMenu.Item class={menuItemClass} onSelect={() => (history = !history)} data-testid="vault-history-open">
                  <span class="flex items-center gap-2"><Icon name="history" class="text-faint" />{t('history')}</span>
                </DropdownMenu.Item>
                {#if !managed}
                  <DropdownMenu.Item class={menuItemClass} onSelect={duplicate} data-testid="vault-duplicate">
                    <span class="flex items-center gap-2"><Icon name="duplicate" class="text-faint" />{t('duplicate')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                  <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={discard} data-testid="vault-trash">
                    <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
                  </DropdownMenu.Item>
                {/if}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
    {/if}
  </header>

  {#if conflict && !editing}
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-xl border border-warn/35 bg-warn-pale px-3 py-2 text-[12.5px] text-ink-2" data-testid="vault-item-conflict">
      <Icon name="conflict" class="shrink-0 text-warn" />
      <span class="min-w-0 flex-1">{t('conflictItem')}</span>
      <button class={buttonClass('secondary', 'xs')} onclick={onCompare}>{t('compare')}</button>
    </div>
  {/if}
  {#if trash}
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-lg bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
      <Icon name="trash" class="shrink-0" />
      <span>{t('inTrashSince', { time: when(entry.at) })}</span>
    </div>
  {/if}
  {#if managed}
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-lg bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
      <Icon name="lock" class="shrink-0" />
      <span>{t('managedHint')}</span>
    </div>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
    {#if editing}
      <ItemForm
        kind={type}
        initial={{ title: item.title, tags: item.tags, notes: item.notes, fields: item.fields, files: item.files ?? [] }}
        edit
        {busy}
        {mod}
        bind:dirty
        onSubmit={save}
        onCancel={() => (editing = false)}
      />
    {:else}
      {#each sections(shownFields) as section (section.title)}
        {#if section.title}<h3 class="mt-3 mb-1 text-[11.5px] font-medium text-faint [overflow-wrap:anywhere]">{section.title}</h3>{/if}
        <dl class={cn('grid gap-1', !section.title && 'mb-3')} data-testid="vault-detail">
          {#each section.fields as field (field.id)}
            {@render fieldRow(field)}
          {/each}
        </dl>
      {/each}
      {#if item.notes}
        <div class="mt-3 py-3">
          <div class="mb-1 text-[12px] text-faint">{t('notes')}</div>
          <p class="text-[13.5px] leading-relaxed whitespace-pre-wrap select-text" data-testid="vault-notes-text">{item.notes}</p>
        </div>
      {:else if !shownFields.length}
        <p class="text-[13px] text-faint">{t('emptyBody')}</p>
      {/if}
      {#if item.files?.length}
        <div class="mt-3 grid gap-1.5" data-testid="vault-attachments">
          <div class="text-[12px] text-faint">{t('attachments')}</div>
          {#each item.files as id (id)}
            <div class="flex items-center gap-2 rounded-lg bg-surface-2 px-3 py-1.5 text-[12.5px]">
              <Icon name="attach" class="shrink-0 text-faint" />
              {#if names[id]}
                <span class="min-w-0 flex-1 truncate">{names[id].name}</span>
                <span class="shrink-0 text-[11.5px] text-faint">{size(names[id].size)}</span>
                <button class={buttonClass('ghost', 'xs')} onclick={() => bridge(() => window.dmsg.filesOpen(id, null)).catch((e) => toast(() => errorText(e)))}>{t('openFile')}</button>
                <button class={buttonClass('ghost', 'xs')} onclick={() => showFile(id)}>{t('showInFiles')}</button>
              {:else}
                <span class="min-w-0 flex-1 truncate text-faint">{names[id] === null ? t('fileMissing') : '…'}</span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
      {#if history}
        <History id={entry.id} head={entry.revision} {device} {unlocked} {editable} onClose={() => (history = false)} />
      {/if}
    {/if}
    <dl class="mt-5 border-t border-line pt-3 text-[11.5px] text-faint" data-testid="vault-identifiers">
      <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <dt>{t('vaultItemId')}</dt>
        <dd class="min-w-0"><Id id={entry.id} copyLabel={t('copyVaultItemId')} disabled={!unlocked} testid="vault-copy-id" /></dd>
      </div>
    </dl>
  </div>

  <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-6 py-2.5 text-[11.5px] text-faint">
    <span class={cn('inline-flex items-center gap-1.5', !entry.synced && 'text-warn')}>
      <Icon name={entry.synced ? 'sync' : 'time'} class="size-3.5" />{entry.synced ? t('itemSynced') : t('itemNotSynced')}
    </span>
    <span title={when(item.createdAt)}>{t('created')} {ago(item.createdAt)}</span>
    <span title={when(item.updatedAt)}>{t('updated')} {ago(item.updatedAt)}</span>
    <button class="hover:text-ink" title={entry.revision} onclick={() => (history = !history)} data-testid="vault-revision">
      {t('revision')} <bdi dir="ltr" class="font-mono">{entry.revision.slice(0, 10)}</bdi>
    </button>
  </footer>
</section>
