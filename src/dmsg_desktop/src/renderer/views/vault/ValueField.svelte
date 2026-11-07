<script lang="ts">
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import FieldActions from './FieldActions.svelte'
  import FieldViewer from './FieldViewer.svelte'
  import { isMultiline, type Field } from './vault'

  let { field, name, context, identity, formatted, unlocked, reference, referenceDisabled, referenceHint, openWebsite }: {
    field: Field; name: string; context: string; identity: string; formatted: string; unlocked: boolean
    reference?: () => Promise<boolean>; referenceDisabled?: boolean; referenceHint?: string; openWebsite?: () => Promise<void>
  } = $props()
  let viewing = $state(false)
  let trigger = $state<HTMLElement | null>(null)
  let copying = $state(false)
  const mono = $derived(['public_key', 'fingerprint'].includes(field.id))
  const ltr = $derived(mono || ['url', 'email', 'phone', 'date', 'month'].includes(field.type))
  const source = $derived(`${identity}:${unlocked}`)

  async function copy() {
    if (!unlocked || copying) return false
    copying = true
    try { return await copyText(field.value, () => t('fieldValueCopied')) }
    finally { copying = false }
  }

  $effect(() => { void source; viewing = false })
</script>

<FieldActions {name} copy={unlocked ? copy : undefined} {reference} {referenceDisabled} {referenceHint}
  openWebsite={unlocked ? openWebsite : undefined} view={unlocked ? (element) => { trigger = element; viewing = true } : undefined}>
  <span class={cn('min-w-0 flex-1 py-1 leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere] select-text',
    isMultiline(field) && 'max-h-64 overflow-auto', mono && 'font-mono text-[12.5px]', field.type === 'url' && 'text-forest')}
    dir={ltr ? 'ltr' : 'auto'} data-testid="vault-field-value">{formatted}</span>
</FieldActions>

{#if viewing && unlocked}
  <FieldViewer bind:open={viewing} {name} {context} value={formatted} {mono} {ltr} {trigger}>
    {#snippet footer()}
      <button class={buttonClass('secondary', 'md')} disabled={copying} onclick={copy} data-testid="vault-large-copy"><Icon name="copy" />{t('copy')}</button>
    {/snippet}
  </FieldViewer>
{/if}
