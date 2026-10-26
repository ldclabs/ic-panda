<script lang="ts">
  import SearchSelect from '../../components/SearchSelect.svelte'
  import Field from '../../components/page/Field.svelte'
  import { workflowText as w } from './workflow-text'
  import { secretText as s } from './text'
  import { ux } from './experience-text'
  import type { SourceItem } from './model'
  let {
    items,
    itemId = $bindable(''),
    fieldId = $bindable(''),
    fields = true,
    concealed = false,
    sharing = false,
    disabled = false,
    onChange,
    testid = 'source'
  }: {
    items: SourceItem[]
    itemId?: string
    fieldId?: string
    fields?: boolean
    concealed?: boolean
    sharing?: boolean
    disabled?: boolean
    onChange?: () => void
    testid?: string
  } = $props()
  const chosen = $derived(items.find((i) => i.id === itemId))
  const options = $derived(
    items.map((i) => ({
      value: i.id,
      label: i.title,
      hint: `${i.id.slice(0, 10)}…${i.blocked || (!sharing && i.type === 'wallet') ? ' · ' + ux('blockedSource') : ''}`,
      disabled: i.blocked || (!sharing && i.type === 'wallet')
    }))
  )
  const fieldOptions = $derived(
    (chosen?.fields ?? [])
      .filter((f) => (concealed ? f.type === 'concealed' : f.type !== 'otp'))
      .map((f) => ({ value: f.id, label: f.label || f.id, hint: f.id }))
  )
</script>

<div class="grid min-w-0 gap-3 sm:grid-cols-2">
  <Field label={w('item')}
    ><SearchSelect
      bind:value={itemId}
      {options}
      label={w('item')}
      placeholder={ux('search')}
      empty={ux('noSources')}
      {disabled}
      testid={`${testid}-item`}
      onChange={() => {
        fieldId = ''
        onChange?.()
      }}
    /></Field
  >
  {#if fields}<Field label={concealed ? s('field') : w('field')}
      ><SearchSelect
        bind:value={fieldId}
        options={fieldOptions}
        label={concealed ? s('field') : w('field')}
        placeholder={w('choose')}
        disabled={disabled || !chosen || chosen.blocked}
        empty={ux('noSources')}
        testid={`${testid}-field`}
        {onChange}
      /></Field
    >{/if}
</div>
{#if !items.some((i) => !i.blocked && (sharing || i.type !== 'wallet'))}<p
    class="text-xs text-muted"
  >
    {ux('noSources')}
  </p>{/if}
