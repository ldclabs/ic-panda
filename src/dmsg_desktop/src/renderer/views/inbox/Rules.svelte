<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import { RadioGroup } from 'bits-ui'
  import Dialog from '../../components/Dialog.svelte'
  import type { Glyph } from '../../components/glyphs'
  import Icon from '../../components/Icon.svelte'
  import { toast } from '../../components/toast.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, errorText } from '../../rpc'

  /** Who may write to this account: nobody, contacts, everyone, or anyone who pays the price per letter. */
  let { open = $bindable(false), policy, onSaved }: { open?: boolean; policy: any; onSaved: (policy: any) => void } = $props()

  const modes: { value: string; label: Label; hint: Label; icon: Glyph }[] = [
    { value: 'closed', label: 'closed', hint: 'closedHint', icon: 'lock' },
    { value: 'contacts', label: 'contactsOnly', hint: 'contactsHint', icon: 'members' },
    { value: 'free', label: 'free', hint: 'freeHint', icon: 'website' },
    { value: 'paid', label: 'paidOnly', hint: 'paidHint', icon: 'coins' }
  ]

  let generation = 0
  let mode = $state('contacts')
  let price = $state('')
  let busy = $state(false)
  let error = $state('')

  $effect(() => {
    if (open) untrack(() => {
      mode = policy?.mode ?? 'contacts'
      price = policy?.price ?? ''
      error = ''
    })
    else generation += 1
  })

  onDestroy(() => { generation += 1 })

  const valid = $derived(mode !== 'paid' || /^\d+$/.test(price.trim()))

  async function save() {
    if (busy) return
    const at = generation
    busy = true
    error = ''
    try {
      const saved = await call('inbox/configure', mode === 'paid' ? { mode, price: price.trim() } : { mode })
      if (at !== generation || !open) return
      toast(() => t('rulesSaved'))
      onSaved(saved)
      open = false
    } catch (e) {
      if (at === generation && open) error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open title={t('contactPolicy')} description={policy ? t('rulesVersion', { version: policy.version }) : t('noPolicy')} icon="mail" testid="rules-dialog">
  <RadioGroup.Root bind:value={mode} class="grid gap-2" aria-label={t('contactPolicy')}>
    {#each modes as option (option.value)}
      <RadioGroup.Item
        value={option.value}
        class="group flex items-start gap-3 rounded-xl border border-line bg-surface px-3.5 py-3 text-start outline-none transition-colors duration-150 hover:border-edge-soft focus-visible:ring-3 focus-visible:ring-focus/35 data-[state=checked]:border-forest data-[state=checked]:bg-pale"
        data-testid={`rules-${option.value}`}
      >
        <span class="grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface-2 text-faint group-data-[state=checked]:border-forest/30 group-data-[state=checked]:text-forest">
          <Icon name={option.icon} />
        </span>
        <span class="min-w-0 flex-1">
          <span class="block text-[13.5px] font-medium">{t(option.label)}</span>
          <span class="block text-[12px] text-muted">{t(option.hint)}</span>
        </span>
        <span class="mt-1 grid size-4 shrink-0 place-items-center rounded-full border border-edge group-data-[state=checked]:border-forest">
          <span class="size-2 scale-0 rounded-full bg-forest transition-transform duration-200 ease-spring group-data-[state=checked]:scale-100"></span>
        </span>
      </RadioGroup.Item>
    {/each}
  </RadioGroup.Root>
  {#if mode === 'paid'}
    <label class="mt-3 grid gap-1 text-[12px] text-faint">
      {t('priceUnits')}
      <input bind:value={price} inputmode="numeric" class={cn('font-mono text-ink tabular-nums', price && !valid && 'border-danger')} data-testid="rules-price" />
    </label>
  {/if}
  {#if error}<p class="mt-2 text-[12.5px] text-danger">{error}</p>{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass('primary')} disabled={busy || !valid} onclick={save} data-testid="rules-save">{t('save')}</button>
  {/snippet}
</Dialog>
