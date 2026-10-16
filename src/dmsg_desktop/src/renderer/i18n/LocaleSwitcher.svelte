<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { isLocale, localeDir, localeNames, locales } from '../../shared/locales'
  import Icon from '../components/Icon.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, setLocale, t } from './index.svelte'

  /** The interface language, an icon button with the website's language menu (`src/dmsg_frontend/src/lib/i18n/LocaleSwitcher.svelte`). */
  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  let busy = $state(false)

  // Arrow keys move through the menu once its content has focus.
  $effect(() => {
    if (open) content?.focus()
  })

  async function pick(next: string) {
    if (busy || !isLocale(next)) return
    busy = true
    try {
      await setLocale(next)
    } catch {
      toast(() => t('languageFailed'))
    } finally {
      busy = false
    }
  }
</script>

<DropdownMenu.Root bind:open dir={localeDir(locale())}>
  <Tooltip content={`${t('language')}: ${localeNames[locale()]}`}>
    {#snippet children(props)}
      <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('language')} aria-busy={busy} data-testid="locale">
        <Icon name="language" />
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
      aria-label={t('language')}
      data-testid="locale-menu"
    >
      <DropdownMenu.RadioGroup value={locale()} onValueChange={pick}>
        {#each locales as code (code)}
          <DropdownMenu.RadioItem value={code} textValue={localeNames[code]} disabled={busy} class={menuItemClass} lang={code} data-value={code}>
            {#snippet children({ checked })}
              <span>{localeNames[code]}</span>
              {#if checked}<Icon name="check" class="size-3.5" />{/if}
            {/snippet}
          </DropdownMenu.RadioItem>
        {/each}
      </DropdownMenu.RadioGroup>
    </DropdownMenu.Content>
  </DropdownMenu.Portal>
</DropdownMenu.Root>
