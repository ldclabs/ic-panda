<script lang="ts">
  import { Combobox } from 'bits-ui'
  import { menuContentClass, menuItemClass } from './ui'

  /**
   * A text field with suggestions, in place of an input with a native
   * `<datalist>`: any text is accepted, and a suggestion fills it in.
   */
  let {
    value = $bindable(''),
    suggestions,
    placeholder,
    testid,
    class: className
  }: {
    value?: string
    suggestions: { value: string; hint?: string }[]
    placeholder?: string
    testid?: string
    class?: string
  } = $props()

  let open = $state(false)
  const shown = $derived.by(() => {
    const query = value.trim().toLowerCase()
    return suggestions.filter((s) => !query || `${s.value} ${s.hint ?? ''}`.toLowerCase().includes(query))
  })
</script>

<Combobox.Root
  type="single"
  bind:open={() => open && shown.length > 0, (next) => (open = next)}
  inputValue={value}
  onValueChange={(next) => next && (value = next)}
  items={suggestions.map((s) => ({ value: s.value, label: s.value }))}
>
  <Combobox.Input
    {placeholder}
    class={className}
    autocomplete="off"
    data-testid={testid}
    oninput={(event) => (value = event.currentTarget.value)}
  />
  <Combobox.Portal>
    <Combobox.Content class={menuContentClass} sideOffset={4} collisionPadding={8}>
      {#each shown as s (s.value)}
        <Combobox.Item value={s.value} label={s.value} class={menuItemClass}>
          <span>{s.value}</span>
          {#if s.hint}<span class="min-w-0 truncate font-mono text-xs text-faint">{s.hint}</span>{/if}
        </Combobox.Item>
      {/each}
    </Combobox.Content>
  </Combobox.Portal>
</Combobox.Root>
