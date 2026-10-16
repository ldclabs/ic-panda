<script lang="ts">
  import { RadioGroup } from 'bits-ui'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import SecretInput from '../../components/SecretInput.svelte'
  import TagInput from '../../components/TagInput.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, code } from '../../rpc'
  import { fields, typeIcon, typeLabel, types, type ItemType } from './vault'

  /**
   * A new item in a dialog: its type, then the fields that type has. A file
   * is imported instead, through the system dialog. The secret goes to the
   * daemon only.
   */
  let {
    open = $bindable(false),
    mod,
    onSaved,
    onImport
  }: { open?: boolean; mod: string; onSaved: (id: string) => void; onImport: () => void } = $props()

  const blank = () => ({ title: '', tags: [] as string[], body: '', username: '', secret: '', url: '' })
  let type = $state<ItemType>('login')
  let draft = $state(blank())
  let busy = $state(false)
  let error = $state('')

  const has = $derived(fields[type])

  $effect(() => {
    if (!open) {
      draft = blank()
      error = ''
    }
  })

  async function save() {
    if (!draft.title.trim() || busy) return
    busy = true
    error = ''
    try {
      const now = Date.now()
      const entry: any = await call('vault/put', {
        item: {
          type,
          title: draft.title.trim(),
          tags: draft.tags,
          body: draft.body,
          username: has.includes('username') ? draft.username : '',
          secret: has.includes('secret') ? draft.secret : '',
          url: has.includes('url') ? draft.url : '',
          favorite: false,
          createdAt: now,
          updatedAt: now
        }
      })
      open = false
      toast(() => t('saved'))
      onSaved(entry.id)
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
      event.preventDefault()
      void save()
    }
  }
</script>

<Dialog bind:open title={t('add')} description={t('newItemHint')} icon="add" testid="vault-new-dialog">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grid gap-4" onkeydown={onKey}>
    <RadioGroup.Root bind:value={() => type, (next) => (type = next as ItemType)} class="grid grid-cols-5 gap-1.5" aria-label={t('type')}>
      {#each types as option (option)}
        <RadioGroup.Item
          value={option}
          class="flex min-w-0 flex-col items-center gap-1 rounded-xl border border-line bg-surface px-1 py-2.5 text-[11.5px] text-muted outline-none transition-colors duration-150 hover:border-edge-soft hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale data-[state=checked]:text-forest"
          data-testid={`vault-type-${option}`}
        >
          <Icon name={typeIcon[option]} class="size-5" />
          <span class="w-full truncate text-center">{t(typeLabel[option])}</span>
        </RadioGroup.Item>
      {/each}
    </RadioGroup.Root>

    {#if type === 'file'}
      <div class="grid place-items-center gap-2 rounded-xl border border-dashed border-edge-soft bg-surface-2 px-6 py-8 text-center">
        <Icon name="upload" class="size-6 text-faint" />
        <p class="max-w-sm text-[12.5px] text-muted">{t('fileImportHint')}</p>
        <button
          class={buttonClass('primary')}
          onclick={() => {
            open = false
            onImport()
          }}
          data-testid="vault-new-file"
        >
          <Icon name="upload" />{t('chooseFile')}
        </button>
      </div>
    {:else}
      <div class="grid gap-3">
        <label class="grid gap-1 text-[12px] text-faint">
          {t('title')}
          <input bind:value={draft.title} class="text-[13.5px] text-ink" data-testid="vault-title" placeholder={t(typeLabel[type])} />
        </label>
        {#if has.includes('url')}
          <label class="grid gap-1 text-[12px] text-faint">{t('url')}<input bind:value={draft.url} class="text-[13.5px] text-ink" spellcheck="false" placeholder="https://" /></label>
        {/if}
        {#if has.includes('username')}
          <label class="grid gap-1 text-[12px] text-faint">{t('username')}<input bind:value={draft.username} class="text-[13.5px] text-ink" spellcheck="false" /></label>
        {/if}
        {#if has.includes('secret')}
          <div class="grid gap-1 text-[12px] text-faint">{t('secret')}<SecretInput bind:value={draft.secret} generate={type === 'login'} testid="vault-new-secret" /></div>
        {/if}
        <div class="grid gap-1 text-[12px] text-faint">{t('tags')}<TagInput bind:tags={draft.tags} placeholder={t('tagsAdd')} /></div>
        <label class="grid gap-1 text-[12px] text-faint">
          {t('body')}
          <textarea bind:value={draft.body} rows={type === 'note' ? 7 : 3} class="text-[13.5px] text-ink"></textarea>
        </label>
      </div>
    {/if}
    {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
  </div>
  {#snippet footer()}
    <span class={cn('min-w-0 flex-1 text-[12px] text-faint', type === 'file' && 'invisible')}>{t('dropHint')}</span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    {#if type !== 'file'}
      <button class={buttonClass('primary')} disabled={busy || !draft.title.trim()} onclick={save} data-testid="vault-add">
        {t('save')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
      </button>
    {/if}
  {/snippet}
</Dialog>
