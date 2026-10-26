<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements'
  import Field from './page/Field.svelte'
  import { cn } from './ui'
  import { textInputTypes, type TextInputType } from './form-values'

  const uid = $props.id()
  let {
    value = $bindable(''),
    label,
    hint,
    error,
    mono = false,
    hideLabel = false,
    type = 'text',
    id = uid,
    class: className,
    ...rest
  }: Omit<HTMLInputAttributes, 'value' | 'type' | 'id' | 'class'> & {
    id?: string
    class?: string
    value?: string
    label: string
    hint?: string
    error?: string
    mono?: boolean
    hideLabel?: boolean
    type?: TextInputType
  } = $props()
  const describedBy = $derived(
    [hint && `${id}-hint`, error && `${id}-error`, rest['aria-describedby']]
      .filter(Boolean)
      .join(' ') || undefined
  )
</script>

<Field
  {hideLabel}
  {label}
  {hint}
  {error}
  for={id}
  hintId={`${id}-hint`}
  errorId={`${id}-error`}
  class={className}
>
  <input
    {...rest}
    {id}
    type={textInputTypes.includes(type) ? type : 'text'}
    bind:value
    aria-describedby={describedBy}
    aria-invalid={!!error || rest['aria-invalid']}
    class={cn(
      'text-[13px] aria-invalid:border-danger read-only:bg-surface-2 read-only:text-muted',
      mono && 'font-mono text-[12.5px]'
    )}
  />
</Field>
