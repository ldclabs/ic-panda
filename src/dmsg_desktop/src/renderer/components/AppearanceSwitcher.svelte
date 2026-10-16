<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { appearances, isAppearance } from '../../shared/appearance'
  import { localeDir } from '../../shared/locales'
  import { appearance, appearanceLabel, setAppearance } from '../appearance.svelte'
  import { locale, t } from '../i18n/index.svelte'
  import Icon from './Icon.svelte'
  import { toast } from './toast.svelte'
  import Tooltip from './Tooltip.svelte'
  import { buttonClass, menuContentClass, menuItemClass } from './ui'

  /** The appearance menu, an icon button: the system's, light or dark. The main process keeps the choice for every window. */
  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  let busy = $state(false)

  // Arrow keys move through the menu once its content has focus.
  $effect(() => {
    if (open) content?.focus()
  })

  async function pick(next: string) {
    if (busy || !isAppearance(next) || next === appearance()) return
    busy = true
    try {
      await setAppearance(next)
    } catch {
      toast(() => t('appearanceFailed'))
    } finally {
      busy = false
    }
  }
</script>

<DropdownMenu.Root bind:open dir={localeDir(locale())}>
  <Tooltip content={`${t('appearance')}: ${t(appearanceLabel[appearance()])}`}>
    {#snippet children(props)}
      <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('appearance')} aria-busy={busy} data-testid="appearance">
        <Icon name={appearance()} />
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>
  <DropdownMenu.Portal>
    <DropdownMenu.Content
      bind:ref={content}
      class={menuContentClass}
      side="top"
      align="end"
      sideOffset={6}
      collisionPadding={8}
      aria-label={t('appearance')}
      data-testid="appearance-menu"
    >
      <DropdownMenu.RadioGroup value={appearance()} onValueChange={pick}>
        {#each appearances as choice (choice)}
          <DropdownMenu.RadioItem value={choice} textValue={t(appearanceLabel[choice])} disabled={busy} class={menuItemClass} data-value={choice}>
            {#snippet children({ checked })}
              <span class="flex items-center gap-2"><Icon name={choice} />{t(appearanceLabel[choice])}</span>
              {#if checked}<Icon name="check" class="size-3.5" />{/if}
            {/snippet}
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
