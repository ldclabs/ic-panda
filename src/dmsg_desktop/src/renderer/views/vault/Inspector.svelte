<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import TagInput from '../../components/TagInput.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, bridge, call, code, size, when } from '../../rpc'
  import SecretField from './SecretField.svelte'
  import { fields, typeIcon, typeLabel, typeOf } from './vault'

  /**
   * One item, opened: its fields with their actions beside them, the body,
   * and where the version stands. Edit turns the fields into a form whose
   * save names the version it replaces; if another device moved on, the
   * daemon keeps both as a conflict.
   */
  let {
    entry,
    conflict,
    trash,
    unlocked,
    mod,
    reload,
    onCompare
  }: {
    entry: any
    conflict: boolean
    trash: boolean
    unlocked: boolean
    mod: string
    reload: () => Promise<void>
    onCompare: () => void
  } = $props()

  let editing = $state<any>(null)
  let busy = $state(false)
  let checked = $state('')

  const item = $derived(entry.item)
  const type = $derived(typeOf(item))
  const has = $derived(fields[type])

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

  const star = () => attempt(() => call('vault/edit', { id: entry.id, base: entry.revision, changes: { favorite: !item.favorite } }).then(reload))
  const discard = () => attempt(() => call(trash ? 'vault/restore' : 'vault/delete', { id: entry.id }).then(reload), trash ? t('restored') : t('movedToTrash'))
  const exportFile = () => attempt(() => bridge(() => window.dmsg.exportFile(entry.id, item.file.name)))
  const verify = () =>
    attempt(async () => {
      const manifest: any = await call('file/verify', { id: entry.id })
      checked = `${t('verified')} · ${size(manifest.size)} · sha256 ${String(manifest.sha256).slice(0, 16)}…`
    })

  /** The form keeps the version it started from: a version synced in meanwhile makes the save a conflict, not an overwrite. */
  function startEdit() {
    editing = { base: entry.revision, title: item.title, tags: [...item.tags], body: item.body, username: item.username, url: item.url, secret: '' }
  }

  const save = () =>
    attempt(async () => {
      const changes: Record<string, unknown> = { title: editing.title.trim(), tags: editing.tags, body: editing.body }
      if (has.includes('username')) changes.username = editing.username
      if (has.includes('url')) changes.url = editing.url
      if (has.includes('secret') && editing.secret) changes.secret = editing.secret
      await call('vault/edit', { id: entry.id, base: editing.base, changes })
      editing = null
      await reload()
    }, t('saved'))

  function onFormKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && editing?.title.trim()) {
      event.preventDefault()
      void save()
    } else if (event.key === 'Escape') {
      event.stopPropagation()
      editing = null
    }
  }
</script>

{#snippet row(label: string)}
  <dt class="pt-2 text-[12px] text-faint @min-[440px]:pt-0">{label}</dt>
{/snippet}

{#snippet copyButton(text: string, label: string)}
  <Tooltip content={label}>
    {#snippet children(props)}
      <button {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={label} onclick={() => copyText(text)}>
        <Icon name="copy" />
      </button>
    {/snippet}
  </Tooltip>
{/snippet}

<section class="@container flex h-full min-w-0 flex-col" aria-label={item.title} data-testid="vault-inspector">
  <header class="flex items-start gap-3 px-6 pt-5 pb-4">
    <span class="grid size-11 shrink-0 place-items-center rounded-xl border border-line bg-surface text-forest shadow-xs [&_svg]:size-5">
      <Icon name={typeIcon[type]} />
    </span>
    <div class="min-w-0 flex-1">
      <h2 class="text-[17px] leading-snug font-semibold [overflow-wrap:anywhere]" data-testid="vault-item-title">{item.title}</h2>
      <div class="mt-1 flex flex-wrap items-center gap-1.5 text-[12px] text-faint">
        <span>{t(typeLabel[type])}</span>
        {#each item.tags as tag, i (i)}
          <span class="rounded-md bg-wash px-1.5 py-px text-[11.5px] text-ink-2">#{tag}</span>
        {/each}
      </div>
    </div>
    {#if unlocked && !editing}
      <div class="flex shrink-0 items-center gap-1">
        {#if !trash}
          <Tooltip content={item.favorite ? t('unstar') : t('star')}>
            {#snippet children(props)}
              <button
                {...props}
                class={buttonClass('ghost', 'md', cn('size-8 px-0', item.favorite && 'text-gem hover:not-disabled:text-gem'))}
                aria-label={item.favorite ? t('unstar') : t('star')}
                aria-pressed={item.favorite}
                disabled={busy}
                onclick={star}
                data-testid="vault-star"
              >
                <Icon name={item.favorite ? 'starred' : 'star'} />
              </button>
            {/snippet}
          </Tooltip>
          <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={startEdit} data-testid="vault-edit-open">
            <Icon name="edit" />{t('edit')}
          </button>
        {:else}
          <button class={buttonClass('secondary', 'md')} disabled={busy} onclick={discard} data-testid="vault-restore">
            <Icon name="restore" />{t('restore')}
          </button>
        {/if}
        {#if !trash}
          <DropdownMenu.Root dir={localeDir(locale())}>
            <Tooltip content={t('moreActions')}>
              {#snippet children(props)}
                <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('moreActions')} data-testid="vault-more">
                  <Icon name="more" />
                </DropdownMenu.Trigger>
              {/snippet}
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-48')} data-testid="vault-more-menu">
                {#if item.file}
                  <DropdownMenu.Item class={menuItemClass} onSelect={exportFile}>
                    <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('exportFile')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Item class={menuItemClass} onSelect={verify}>
                    <span class="flex items-center gap-2"><Icon name="verified" class="text-faint" />{t('verifyFile')}</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                {/if}
                <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={discard} data-testid="vault-trash">
                  <span class="flex items-center gap-2"><Icon name="trash" />{t('moveToTrash')}</span>
                </DropdownMenu.Item>
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
    <div class="mx-6 mb-3 flex items-center gap-2.5 rounded-xl border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
      <Icon name="trash" class="shrink-0" />
      <span>{t('inTrash')}</span>
    </div>
  {/if}

  <div class="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
    {#if editing}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="grid gap-3" data-testid="vault-edit" onkeydown={onFormKey}>
        <label class="grid gap-1 text-[12px] text-faint">{t('title')}<input bind:value={editing.title} class="text-[13.5px] text-ink" /></label>
        {#if has.includes('username')}
          <label class="grid gap-1 text-[12px] text-faint">{t('username')}<input bind:value={editing.username} class="text-[13.5px] text-ink" /></label>
        {/if}
        {#if has.includes('url')}
          <label class="grid gap-1 text-[12px] text-faint">{t('url')}<input bind:value={editing.url} class="text-[13.5px] text-ink" spellcheck="false" /></label>
        {/if}
        {#if has.includes('secret')}
          <div class="grid gap-1 text-[12px] text-faint">{t('secret')}<SecretInput bind:value={editing.secret} placeholder={t('newSecret')} generate /></div>
        {/if}
        <div class="grid gap-1 text-[12px] text-faint">{t('tags')}<TagInput bind:tags={editing.tags} placeholder={t('tagsAdd')} /></div>
        <label class="grid gap-1 text-[12px] text-faint">{t('body')}<textarea bind:value={editing.body} rows="6" class="text-[13.5px] text-ink"></textarea></label>
        <div class="flex items-center justify-end gap-2 pt-1">
          <button class={buttonClass('ghost')} onclick={() => (editing = null)}>{t('cancel')}</button>
          <button class={buttonClass('primary')} disabled={busy || !editing.title.trim()} onclick={save} data-testid="vault-save">
            {t('save')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
          </button>
        </div>
      </div>
    {:else}
      {#if has.length || item.file}
        <dl class="divide-y divide-line rounded-xl border border-line bg-surface px-4 shadow-xs" data-testid="vault-detail">
          {#if item.username}
            <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
              {@render row(t('username'))}
              <dd class="flex min-w-0 items-center gap-1.5">
                <span class="min-w-0 flex-1 truncate select-text">{item.username}</span>
                {@render copyButton(item.username, t('copyUsername'))}
              </dd>
            </div>
          {/if}
          {#if has.includes('secret') && item.secret}
            <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
              {@render row(t('secret'))}
              <dd class="flex min-w-0 py-0.5">
                {#key entry.revision}
                  <SecretField id={entry.id} unlocked={unlocked && !trash} />
                {/key}
              </dd>
            </div>
          {/if}
          {#if item.url}
            <div class="grid gap-x-4 py-1.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
              {@render row(t('url'))}
              <dd class="flex min-w-0 items-center gap-1.5">
                <span class="min-w-0 flex-1 truncate text-forest select-text" dir="ltr">{item.url}</span>
                {@render copyButton(item.url, t('copy'))}
              </dd>
            </div>
          {/if}
          {#if item.file}
            <div class="grid gap-x-4 py-2.5 @min-[440px]:grid-cols-[112px_1fr] @min-[440px]:items-center">
              {@render row(t('typeFile'))}
              <dd class="min-w-0">
                <div class="truncate">{item.file.name}</div>
                <div class="text-[12px] text-faint">{size(item.file.size)}{item.file.mime ? ` · ${item.file.mime}` : ''}</div>
                {#if checked}<div class="mt-1 flex items-center gap-1 text-[12px] text-ok" data-testid="file-checked"><Icon name="verified" class="size-3.5" />{checked}</div>{/if}
              </dd>
            </div>
          {/if}
        </dl>
      {/if}
      {#if item.body}
        <div class={cn('rounded-xl border border-line bg-surface px-4 py-3 shadow-xs', (has.length || item.file) && 'mt-3')}>
          {#if has.length || item.file}<div class="mb-1 text-[12px] text-faint">{t('body')}</div>{/if}
          <p class="text-[13.5px] leading-relaxed whitespace-pre-wrap select-text" data-testid="vault-body">{item.body}</p>
        </div>
      {:else if !has.length && !item.file}
        <p class="text-[13px] text-faint">{t('emptyBody')}</p>
      {/if}
    {/if}
  </div>

  <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-line px-6 py-2.5 text-[11.5px] text-faint">
    <span class={cn('inline-flex items-center gap-1.5', !entry.synced && 'text-warn')}>
      <Icon name={entry.synced ? 'sync' : 'time'} class="size-3.5" />{entry.synced ? t('synced') : t('notSynced')}
    </span>
    <span title={when(item.updatedAt)}>{t('updated')} {ago(item.updatedAt)}</span>
    <span title={entry.revision}>{t('revision')} <bdi dir="ltr" class="font-mono">{entry.revision.slice(0, 10)}</bdi></span>
  </footer>
</section>
