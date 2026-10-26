<script lang="ts" module>
  export interface SearchOption {
    value: string
    label: string
    hint?: string
    disabled?: boolean
  }
</script>

<script lang="ts">
  import { Combobox } from 'bits-ui'
  import Icon from './Icon.svelte'
  import { menuContentClass, menuItemClass } from './ui'
  import { t, locale } from '../i18n/index.svelte'
  import { localeDir } from '../../shared/locales'
  let {
    value = $bindable(''),
    options,
    label,
    placeholder = '',
    disabled = false,
    onChange,
    testid,
    empty
  }: {
    value?: string
    options: SearchOption[]
    label: string
    placeholder?: string
    disabled?: boolean
    onChange?: (value: string) => void
    testid?: string
    empty?: string
  } = $props()
  let query = $state(''),
    open = $state(false)
  const chosen = $derived(options.find((o) => o.value === value))
  const shown = $derived(
    options.filter(
      (o) =>
        !query ||
        `${o.label} ${o.hint ?? ''} ${o.value}`
          .toLowerCase()
          .includes(query.toLowerCase())
    )
  )
  function pick(next: string) {
    if (!next || options.find((o) => o.value === next)?.disabled) return
    value = next
    query = ''
    onChange?.(next)
  }
</script>

<Combobox.Root
  type="single"
  {value}
  onValueChange={pick}
  bind:open
  onOpenChange={(next) => {
    if (!next) query = ''
  }}
  inputValue={open ? query : (chosen?.label ?? '')}
  items={options}
  {disabled}
>
  <div class="relative min-w-0">
    <Combobox.Input
      aria-label={label}
      {placeholder}
      autocomplete="off"
      onclick={() => (open = true)}
      class="pe-9 text-[13px]"
      oninput={(e) => {
        query = e.currentTarget.value
        open = true
      }}
      data-testid={testid}
    />
    <Combobox.Trigger
      aria-label={label}
      class="absolute end-0 top-0 grid size-8 place-items-center rounded-lg text-muted outline-none focus-visible:ring-2 focus-visible:ring-focus"
      ><Icon name="chevron-down" /></Combobox.Trigger
    >
  </div>
  <Combobox.Portal
    ><Combobox.Content
      dir={localeDir(locale())}
      class={menuContentClass}
      sideOffset={4}
      collisionPadding={8}
      data-testid={testid && `${testid}-menu`}
    >
      {#each shown as option (option.value)}<Combobox.Item
          value={option.value}
          label={option.label}
          disabled={option.disabled}
          class={menuItemClass}
          data-value={option.value}
          ><span class="min-w-0"
            ><span class="block">{option.label}</span>{#if option.hint}<span
                class="block text-xs text-muted">{option.hint}</span
              >{/if}</span
          >{#if value === option.value}<Icon name="check" />{/if}</Combobox.Item
        >{:else}<p class="px-3 py-2 text-sm text-muted" role="status">
          {empty ?? t('noResults')}
        </p>{/each}
    </Combobox.Content></Combobox.Portal
  >
</Combobox.Root>
