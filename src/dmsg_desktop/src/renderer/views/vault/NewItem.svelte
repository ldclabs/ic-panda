<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import Notice from '../../components/page/Notice.svelte'
  import { RadioGroup } from 'bits-ui'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { call, errorText } from '../../rpc'
  import ItemForm from './ItemForm.svelte'
  import { typeIcon, typeLabel, types, type Field, type ItemType, type Templates } from './vault'

  /**
   * A new item in a dialog: its type, then the form with that type's
   * fields, to which people add their own. Values go to the daemon only;
   * closing with something typed asks first.
   */
  let {
    open = $bindable(false),
    mod,
    templates,
    onSaved
  }: { open?: boolean; mod: string; templates: Templates | null; onSaved: (id: string) => void } = $props()

  let type = $state<ItemType>('login')
  let busy = $state(false)
  let error = $state('')
  let dirty = $state(false)
  let valid = $state(false)
  let discardTo = $state<ItemType | 'close' | null>(null)
  let keepButton = $state<HTMLButtonElement | null>(null)
  let form = $state<ItemForm | null>(null)
  let alive = true
  onDestroy(() => { alive = false })

  const initial = $derived({
    title: '',
    tags: [] as string[],
    notes: '',
    files: [] as string[],
    fields: (templates?.[type]?.fields ?? []).map((f): Field => ({ id: f.id, type: f.type, label: '', section: '', value: '' }))
  })

  $effect(() => {
    if (!open) {
      error = ''
      discardTo = null
      dirty = false
    }
  })

  function apply(next: ItemType | 'close') {
    discardTo = null
    dirty = false
    error = ''
    if (next === 'close') open = false
    else type = next
  }

  async function change(next: ItemType | 'close') {
    if (busy || next === type) return
    if (!dirty) return apply(next)
    discardTo = next
    await tick()
    keepButton?.focus()
  }

  async function finishDiscard(discard: boolean) {
    if (discard && discardTo) apply(discardTo)
    else discardTo = null
    await tick()
    if (open) form?.focusTitle()
  }

  function close(next: boolean) {
    if (next) open = true
    else if (discardTo) void finishDiscard(false)
    else void change('close')
  }

  async function save(changes: Record<string, unknown>) {
    if (busy || discardTo) return
    busy = true
    error = ''
    try {
      const entry: any = await call('vault/create', { type, changes })
      if (!alive) return
      dirty = false
      open = false
      toast(() => t('saved'))
      onSaved(entry.id)
    } catch (e) {
      if (alive) error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open={() => open, close} title={t('add')} description={t('newItemHint')} icon="add" wide testid="vault-new-dialog"
  dismissible={!busy} onOpenAutoFocus={(event) => { if (form) { event.preventDefault(); form.focusTitle() } }}>
  <fieldset class="grid min-w-0 gap-4" disabled={busy || !!discardTo} aria-busy={busy}>
    <RadioGroup.Root bind:value={() => type, (next) => void change(next as ItemType)} class="grid grid-cols-4 gap-1.5 sm:grid-cols-6" aria-label={t('type')}>
      {#each types as option (option)}
        <RadioGroup.Item
          value={option}
          class="flex min-w-0 flex-col items-center gap-1 rounded-xl border border-line bg-surface px-1 py-2 text-[11.5px] text-muted outline-none transition-colors duration-150 hover:border-edge-soft hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale data-[state=checked]:text-forest"
          data-testid={`vault-type-${option}`}
        >
          <Icon name={typeIcon[option]} class="size-5" />
          <span class="w-full truncate text-center">{t(typeLabel[option])}</span>
        </RadioGroup.Item>
      {/each}
    </RadioGroup.Root>
    {#if templates}
      {#key type}
        <ItemForm bind:this={form} bind:dirty bind:valid kind={type} {initial} {busy} {mod} footer={false} onSubmit={save} onCancel={() => close(false)} />
      {/key}
    {:else}
      <p class="shimmer text-[13px]">{t('loading')}</p>
    {/if}
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  </fieldset>
  {#snippet footer()}
    {#if discardTo}
      <span class="min-w-0 flex-1 text-[12.5px] text-warn" role="status" data-testid="vault-new-leave">{t('discardNew')}{#if discardTo !== 'close'} · {t(typeLabel[type])} <Icon name="chevron-end" class="inline size-3" /> {t(typeLabel[discardTo])}{/if}</span>
      <button bind:this={keepButton} class={buttonClass('secondary')} onclick={() => finishDiscard(false)}>{t('keepEditing')}</button>
      <button class={buttonClass('danger')} onclick={() => finishDiscard(true)} data-testid="vault-new-discard">{t('discard')}</button>
    {:else}
      <span class="min-w-0 flex-1"></span>
      <button class={buttonClass('ghost')} disabled={busy} onclick={() => close(false)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy || !templates || !valid} onclick={() => form?.submit()} data-testid="vault-add">
        {t('save')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
      </button>
    {/if}
  {/snippet}
</Dialog>
