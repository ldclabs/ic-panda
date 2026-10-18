<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { tick, untrack } from 'svelte'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import TagInput from '../../components/TagInput.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { bridge, call, code, errorText, size } from '../../rpc'
  import FilePicker from './FilePicker.svelte'
  import {
    drafts as toDrafts,
    fieldChanges,
    fieldLabel,
    fieldName,
    fieldTypeLabel,
    fieldTypes,
    generates,
    isHidden,
    isMultiline,
    type Draft,
    type Field,
    type FieldType,
    type ItemType
  } from './vault'

  /**
   * An item's form, new or edited: the title, its fields in order (each
   * with a label, a section and a value fit for its type), notes, tags and
   * attached library files. In an edit a hidden value already saved stays
   * in the daemon: it is kept, replaced or cleared, never loaded here.
   * `dirty` tells the view whether leaving would lose something.
   */
  let {
    kind,
    initial,
    edit = false,
    busy = false,
    mod,
    dirty = $bindable(false),
    onSubmit,
    onCancel,
    footer = true
  }: {
    kind: ItemType
    initial: { title: string; tags: string[]; notes: string; fields: Field[]; files: string[] }
    edit?: boolean
    busy?: boolean
    mod: string
    dirty?: boolean
    onSubmit: (changes: Record<string, unknown>) => void
    onCancel: () => void
    footer?: boolean
  } = $props()

  // The form starts from `initial` once; later changes of it are the view's.
  const start = untrack(() => ({ ...initial, edit }))
  let title = $state(start.title)
  let tags = $state<string[]>([...start.tags])
  let notes = $state(start.notes)
  let files = $state<string[]>([...start.files])
  let fields = $state<Draft[]>(start.edit ? toDrafts(start.fields) : start.fields.map((f) => ({ ...f })))
  let sectionOpen = $state<Set<number>>(new Set(start.fields.flatMap((f, i) => (f.section ? [i] : []))))
  let names = $state<Record<string, string>>({})
  let picking = $state(false)
  let list = $state<HTMLElement | null>(null)

  const snapshot = (value: unknown) => JSON.stringify(value)
  const first = snapshot({ title: start.title, tags: start.tags, notes: start.notes, files: start.files, fields: start.edit ? toDrafts(start.fields) : start.fields })
  $effect(() => {
    dirty = snapshot({ title, tags, notes, files, fields }) !== first
  })

  // Attached files by name; one no longer in the library shows its ID.
  $effect(() => {
    for (const id of files)
      if (!(id in names))
        void call('files/show', { id })
          .then((details: any) => (names = { ...names, [id]: `${details.file.name} · ${size(details.file.size)}` }))
          .catch(() => (names = { ...names, [id]: t('fileMissing') }))
  })

  export function submit() {
    if (!title.trim() || busy) return
    onSubmit({ title: title.trim(), tags, notes, files, fields: fieldChanges(fields) })
  }

  async function add(type: FieldType) {
    fields = [...fields, { id: '', type, label: '', section: '', value: '' }]
    await tick()
    list?.querySelector<HTMLInputElement>(`[data-index="${fields.length - 1}"] input[data-label]`)?.focus()
  }

  function move(i: number, by: number) {
    const j = i + by
    if (j < 0 || j >= fields.length) return
    const next = [...fields]
    ;[next[i], next[j]] = [next[j], next[i]]
    fields = next
  }

  function remove(i: number) {
    fields = fields.filter((_, k) => k !== i)
  }

  function set(i: number, patch: Partial<Draft>) {
    fields = fields.map((f, k) => (k === i ? { ...f, ...patch } : f))
  }

  async function scan(i: number, source: 'file' | 'clipboard') {
    try {
      const found = await bridge<{ uri: string } | null>(() => window.dmsg.vaultOtpScan(source))
      if (found) set(i, { value: found.uri, hiddenState: 'change' })
    } catch (e) {
      toast(() => (code(e) === 'NOT_FOUND' ? t('noQrFound') : errorText(e)))
    }
  }

  async function keygen() {
    try {
      const key: any = await call('vault/ssh_keygen', { comment: title.trim() })
      for (const [id, value] of [
        ['private_key', key.private_key],
        ['public_key', key.public_key],
        ['fingerprint', key.fingerprint]
      ]) {
        const at = fields.findIndex((f) => f.id === id)
        if (at >= 0) set(at, { value, hiddenState: isHidden(fields[at]) ? 'change' : undefined })
      }
    } catch (e) {
      toast(() => errorText(e))
    }
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      submit()
    } else if (event.key === 'Escape' && !picking) {
      event.stopPropagation()
      onCancel()
    }
  }

  const inputType: Partial<Record<FieldType, string>> = { url: 'url', email: 'email', phone: 'tel', date: 'date', month: 'month' }
  const placeholder = (field: Draft) => (fieldLabel[field.id] ? t(fieldLabel[field.id]) : t('fieldLabelHint'))
</script>

{#snippet value(field: Draft, i: number)}
  {#if isHidden(field) && field.hiddenState === 'keep'}
    <div class="flex items-center gap-1.5 rounded-lg border border-dashed border-edge-soft px-2.5 py-1">
      <span class="min-w-0 flex-1 font-mono text-[12.5px] tracking-[0.18em] text-faint">••••••••</span>
      <span class="text-[11.5px] text-faint">{t('savedValue')}</span>
      <button type="button" class={buttonClass('ghost', 'xs')} onclick={() => set(i, { hiddenState: 'change' })} data-testid="field-change">{t('change')}</button>
      <button type="button" class={buttonClass('ghost', 'xs', 'text-danger')} onclick={() => set(i, { hiddenState: 'clear' })} data-testid="field-clear">{t('clearValue')}</button>
    </div>
  {:else if isHidden(field) && field.hiddenState === 'clear'}
    <div class="flex items-center gap-1.5 rounded-lg border border-dashed border-danger/40 bg-danger-pale px-2.5 py-1 text-[12.5px] text-danger">
      <span class="min-w-0 flex-1">{t('willClear')}</span>
      <button type="button" class={buttonClass('ghost', 'xs')} onclick={() => set(i, { hiddenState: 'keep' })}>{t('undo')}</button>
    </div>
  {:else if field.type === 'otp'}
    <div class="grid gap-1.5">
      <SecretInput bind:value={() => field.value, (next) => set(i, { value: next })} placeholder={t('otpHint')} testid={`field-${field.id || i}`} />
      <div class="flex flex-wrap gap-1">
        <button type="button" class={buttonClass('ghost', 'xs')} onclick={() => scan(i, 'file')} data-testid="otp-scan-file"><Icon name="scan" />{t('scanQrImage')}</button>
        <button type="button" class={buttonClass('ghost', 'xs')} onclick={() => scan(i, 'clipboard')} data-testid="otp-scan-clipboard"><Icon name="paste" />{t('scanQrClipboard')}</button>
      </div>
    </div>
  {:else if field.type === 'concealed'}
    <SecretInput
      bind:value={() => field.value, (next) => set(i, { value: next })}
      placeholder={field.hiddenState === 'change' ? t('newSecretValue') : ''}
      generate={generates(field)}
      pin={field.id === 'pin'}
      multiline={isMultiline(field)}
      testid={`field-${field.id || i}`}
    />
  {:else if field.type === 'text' && isMultiline(field)}
    <textarea
      value={field.value}
      oninput={(e) => set(i, { value: e.currentTarget.value })}
      rows={Math.min(10, Math.max(2, field.value.split('\n').length))}
      spellcheck="false"
      class={cn('text-[13px] text-ink', ['public_key', 'fingerprint'].includes(field.id) && 'font-mono text-[12px]')}
      data-testid={`field-${field.id || i}`}
    ></textarea>
  {:else}
    <input
      type={inputType[field.type] ?? 'text'}
      value={field.value}
      oninput={(e) => set(i, { value: e.currentTarget.value })}
      spellcheck="false"
      dir={field.type === 'url' || field.type === 'email' ? 'ltr' : undefined}
      placeholder={field.type === 'url' ? 'https://' : ''}
      class={cn('text-[13.5px] text-ink', ['fingerprint', 'port'].includes(field.id) && 'font-mono text-[12.5px]')}
      data-testid={`field-${field.id || i}`}
    />
  {/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="grid gap-3" onkeydown={onKey} data-testid="vault-form">
  <label class="grid gap-1 text-[12px] text-faint">
    {t('title')}
    <input bind:value={title} class="text-[13.5px] text-ink" data-testid="vault-title" />
  </label>

  {#if kind === 'ssh'}
    <div class="flex items-center gap-2 rounded-xl border border-line bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
      <Icon name="ssh" class="shrink-0" />
      <span class="min-w-0 flex-1">{t('sshKeygenHint')}</span>
      <button type="button" class={buttonClass('secondary', 'sm')} onclick={keygen} data-testid="ssh-keygen"><Icon name="generate" />{t('sshKeygen')}</button>
    </div>
  {/if}

  <div bind:this={list} class="grid gap-2.5" data-testid="vault-form-fields">
    {#each fields as field, i (i)}
      <div class="grid gap-1 rounded-xl border border-line bg-surface px-2.5 py-2" data-index={i} data-field={field.id} data-type={field.type}>
        <div class="flex items-center gap-1.5">
          <input
            value={field.label}
            oninput={(e) => set(i, { label: e.currentTarget.value })}
            placeholder={placeholder(field)}
            aria-label={t('fieldLabel')}
            class="h-6 min-w-0 flex-1 border-transparent bg-transparent px-1 text-[12px] font-medium text-ink-2 shadow-none placeholder:text-ink-2 hover:border-edge-soft focus:border-forest"
            data-label
          />
          <span class="shrink-0 rounded bg-wash px-1.5 py-px text-[11px] text-faint">{t(fieldTypeLabel[field.type])}</span>
          <DropdownMenu.Root dir={localeDir(locale())}>
            <DropdownMenu.Trigger class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('moreActions')} data-testid="field-menu">
              <Icon name="more" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={4} class={cn(menuContentClass, 'min-w-44')}>
                <DropdownMenu.Item class={menuItemClass} disabled={i === 0} onSelect={() => move(i, -1)}>{t('moveUp')}</DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} disabled={i === fields.length - 1} onSelect={() => move(i, 1)}>{t('moveDown')}</DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} onSelect={() => (sectionOpen = new Set([...sectionOpen, i]))}>{t('setSection')}</DropdownMenu.Item>
                <DropdownMenu.Separator class="my-1 h-px bg-line" />
                <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => remove(i)} data-testid="field-remove">{t('removeField')}</DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        </div>
        {@render value(field, i)}
        {#if sectionOpen.has(i)}
          <label class="flex items-center gap-2 text-[11.5px] text-faint">
            {t('section')}
            <input value={field.section} oninput={(e) => set(i, { section: e.currentTarget.value })} class="h-6 min-w-0 flex-1 text-[12px] text-ink" />
          </label>
        {/if}
      </div>
    {/each}
  </div>

  <DropdownMenu.Root dir={localeDir(locale())}>
    <DropdownMenu.Trigger class={buttonClass('ghost', 'sm', 'justify-self-start text-forest')} data-testid="field-add">
      <Icon name="add" />{t('addField')}
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content align="start" sideOffset={4} class={cn(menuContentClass, 'min-w-48')} data-testid="field-add-menu">
        {#each fieldTypes as type (type)}
          <DropdownMenu.Item class={menuItemClass} onSelect={() => add(type)} data-value={type}>{t(fieldTypeLabel[type])}</DropdownMenu.Item>
        {/each}
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>

  <label class="grid gap-1 text-[12px] text-faint">
    {t('notes')}
    <textarea bind:value={notes} rows={kind === 'note' ? 8 : 3} class="text-[13.5px] text-ink" data-testid="vault-notes"></textarea>
  </label>
  <div class="grid gap-1 text-[12px] text-faint">{t('tags')}<TagInput bind:tags placeholder={t('tagsAdd')} maxLength={40} /></div>

  <div class="grid gap-1.5 text-[12px] text-faint">
    {t('attachments')}
    {#each files as id (id)}
      <div class="flex items-center gap-2 rounded-lg border border-line bg-surface px-2.5 py-1.5 text-[12.5px] text-ink">
        <Icon name="attach" class="shrink-0 text-faint" />
        <span class="min-w-0 flex-1 truncate">{names[id] ?? '…'}</span>
        <Tooltip content={t('remove')}>
          {#snippet children(props)}
            <button {...props} type="button" class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('remove')} onclick={() => (files = files.filter((f) => f !== id))}>
              <Icon name="close" />
            </button>
          {/snippet}
        </Tooltip>
      </div>
    {/each}
    {#if files.length < 20}
      <button type="button" class={buttonClass('ghost', 'sm', 'justify-self-start text-forest')} onclick={() => (picking = true)} data-testid="attach-file">
        <Icon name="attach" />{t('attachFile')}
      </button>
    {/if}
  </div>

  {#if footer}
    <div class="flex items-center justify-end gap-2 pt-1">
      <button type="button" class={buttonClass('ghost')} onclick={onCancel}>{t('cancel')}</button>
      <button type="button" class={buttonClass('primary')} disabled={busy || !title.trim()} onclick={submit} data-testid="vault-save">
        {t('save')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
      </button>
    </div>
  {/if}
</div>

<FilePicker bind:open={picking} taken={files} onPick={(id) => (files = [...files, id])} />
