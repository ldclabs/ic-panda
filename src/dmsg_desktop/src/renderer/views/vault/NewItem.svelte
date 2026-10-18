<script lang="ts">
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
  let leaving = $state(false)
  let form = $state<ItemForm | null>(null)

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
      leaving = false
      dirty = false
    }
  })

  /** Closing asks first when something was typed; only Discard then closes. */
  function close(next: boolean) {
    if (next) open = true
    else if (dirty) leaving = true
    else open = false
  }

  async function save(changes: Record<string, unknown>) {
    busy = true
    error = ''
    try {
      const entry: any = await call('vault/create', { type, changes })
      dirty = false
      open = false
      toast(() => t('saved'))
      onSaved(entry.id)
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open={() => open, close} title={t('add')} description={t('newItemHint')} icon="add" wide testid="vault-new-dialog">
  <div class="grid gap-4">
    <RadioGroup.Root bind:value={() => type, (next) => (type = next as ItemType)} class="grid grid-cols-4 gap-1.5 sm:grid-cols-6" aria-label={t('type')}>
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
        <ItemForm bind:this={form} bind:dirty kind={type} {initial} {busy} {mod} footer={false} onSubmit={save} onCancel={() => close(false)} />
      {/key}
    {:else}
      <p class="shimmer text-[13px]">{t('loading')}</p>
    {/if}
    {#if error}<p class="text-[12.5px] text-danger">{error}</p>{/if}
  </div>
  {#snippet footer()}
    {#if leaving}
      <span class="min-w-0 flex-1 text-[12.5px] text-warn" data-testid="vault-new-leave">{t('discardNew')}</span>
      <button class={buttonClass('ghost')} onclick={() => (leaving = false)}>{t('keepEditing')}</button>
      <button class={buttonClass('danger')} onclick={() => ((dirty = false), (open = false))} data-testid="vault-new-discard">{t('discard')}</button>
    {:else}
      <span class="min-w-0 flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => close(false)}>{t('cancel')}</button>
      <button class={buttonClass('primary')} disabled={busy || !templates} onclick={() => form?.submit()} data-testid="vault-add">
        {t('save')}<kbd class="font-mono text-[10.5px] opacity-75">{mod}↵</kbd>
      </button>
    {/if}
  {/snippet}
</Dialog>
