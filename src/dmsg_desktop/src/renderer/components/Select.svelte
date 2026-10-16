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
  import Icon from './Icon.svelte'
  import { cn, menuContentClass, menuItemClass } from './ui'

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
    class={cn(
      'group inline-flex h-8 max-w-full shrink-0 items-center justify-between gap-2 rounded-lg border border-edge bg-surface ps-2.5 pe-2 text-start text-[13px] text-ink outline-none transition-[border-color,box-shadow] duration-150 hover:not-disabled:border-forest/60 focus-visible:border-forest focus-visible:ring-3 focus-visible:ring-forest/18 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:border-forest',
      className
    )}
    disabled={disabled || !options.length}
    aria-label={label}
    data-testid={testid}
  >
    <span class={cn('max-w-[260px] truncate', chosen?.mono && 'font-mono text-xs', !chosen && 'text-faint')}>{chosen?.label ?? placeholder}</span>
    <Icon name="chevron-down" class="text-faint transition-transform duration-150 group-data-[state=open]:rotate-180 motion-reduce:transition-none" />
  </DropdownMenu.Trigger>
  <DropdownMenu.Portal>
    <DropdownMenu.Content
      bind:ref={content}
      class={menuContentClass}
      align="start"
      sideOffset={4}
      collisionPadding={8}
      aria-label={label}
      data-testid={testid && `${testid}-menu`}
    >
      <DropdownMenu.RadioGroup {value} onValueChange={pick}>
        {#each options as option (option.value)}
          <DropdownMenu.RadioItem value={option.value} textValue={option.label} class={menuItemClass} data-value={option.value}>
            {#snippet children({ checked })}
              <span class={option.mono ? 'font-mono text-xs' : ''}>{option.label}</span>
              {#if checked}<Icon name="check" class="size-3.5" />{/if}
            {/snippet}
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
