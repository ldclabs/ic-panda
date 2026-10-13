<script lang="ts" module>
  export interface Option {
    value: string
    label: string
    /** An identifier rather than words, set in the monospace face. */
    mono?: boolean
  }
</script>

<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../shared/locales'
  import { locale } from '../i18n/index.svelte'

  /**
   * A choice among options on the bits-ui dropdown menu, in place of a native
   * `<select>`: the trigger shows the chosen label, the menu checks it.
   * `value` binds as a select's would; `onChange` runs after a pick. With a
   * `testid`, the menu is `${testid}-menu` and each option carries
   * `data-value`, since the menu renders outside the trigger.
   */
  let {
    value = $bindable(''),
    options,
    placeholder = '',
    label,
    disabled = false,
    onChange,
    testid,
    class: className = ''
  }: {
    value?: string
    options: Option[]
    placeholder?: string
    label?: string
    disabled?: boolean
    onChange?: (value: string) => void
    testid?: string
    class?: string
  } = $props()

  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  const chosen = $derived(options.find((option) => option.value === value))

  // Arrow keys move through the menu once its content has focus.
  $effect(() => {
    if (open) content?.focus()
  })

  function pick(next: string) {
    value = next
    onChange?.(next)
  }
</script>

<DropdownMenu.Root bind:open dir={localeDir(locale())}>
  <DropdownMenu.Trigger
    class={['select', className]}
    disabled={disabled || !options.length}
    aria-label={label}
    data-testid={testid}
  >
    <span class={['select-value', chosen?.mono && 'mono', !chosen && 'muted']}>{chosen?.label ?? placeholder}</span>
    <svg class="select-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" aria-hidden="true">
      <path d="m6 9 6 6 6-6" />
    </svg>
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content
      bind:ref={content}
      class="select-menu"
      align="start"
      sideOffset={4}
      collisionPadding={8}
      aria-label={label}
      data-testid={testid && `${testid}-menu`}
    >
      <DropdownMenu.RadioGroup {value} onValueChange={pick}>
        {#each options as option (option.value)}
          <DropdownMenu.RadioItem value={option.value} textValue={option.label} class="select-item" data-value={option.value}>
            {#snippet children({ checked })}
              <span class={option.mono ? 'mono' : ''}>{option.label}</span>
              {#if checked}
                <svg class="select-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                  <path d="M4 12.5 9.5 18 20 6" />
                </svg>
              {/if}
            {/snippet}
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>

<style>
  :global(.select) {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    max-width: 100%;
    padding: 6px 10px;
    text-align: start;
  }
  :global(.select:focus-visible) {
    outline: 2px solid var(--green);
    outline-offset: 1px;
  }
  .select-value {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .select-chevron {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    opacity: 0.6;
    transition: transform 150ms;
  }
  :global(.select[data-state='open']) .select-chevron {
    transform: rotate(180deg);
  }
  .select-check {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .select-chevron {
      transition: none;
    }
  }
</style>
