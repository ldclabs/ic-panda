<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements'
  import Field from './page/Field.svelte'
  import { cn } from './ui'
  const uid = $props.id()
  let {
    value = $bindable(''),
    label,
    hint,
    error,
    mono = false,
    id = uid,
    rows = 3,
    class: className,
    ...rest
  }: Omit<HTMLTextareaAttributes, 'value' | 'id' | 'class'> & {
    id?: string
    class?: string
    value?: string
    label: string
    hint?: string
    error?: string
    mono?: boolean
  } = $props()
  const describedBy = $derived(
    [hint && `${id}-hint`, error && `${id}-error`, rest['aria-describedby']]
      .filter(Boolean)
      .join(' ') || undefined
  )
</script>

<Field
  {label}
  {hint}
  {error}
  for={id}
  hintId={`${id}-hint`}
  errorId={`${id}-error`}
  class={className}
>
  <textarea
    {...rest}
    {id}
    {rows}
    bind:value
    aria-describedby={describedBy}
    aria-invalid={!!error || rest['aria-invalid']}
    class={cn(
      'text-[13px] aria-invalid:border-danger read-only:bg-surface-2 read-only:text-muted',
      mono && 'font-mono text-[12.5px]'
    )}></textarea>
</Field>
