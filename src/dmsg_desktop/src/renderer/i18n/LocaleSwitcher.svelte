<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { isLocale, localeDir, localeNames, locales } from '../../shared/locales'
  import { locale, setLocale, t } from './index.svelte'

  /** The interface language, on the website's language menu (`src/dmsg_frontend/src/lib/i18n/LocaleSwitcher.svelte`). */
  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  let busy = $state(false)
  let failed = $state(false)
  const errorId = $props.id()

  // Arrow keys move through the menu once its content has focus.
  $effect(() => {
    if (open) content?.focus()
  })

  async function pick(next: string) {
    if (busy || !isLocale(next)) return
    busy = true
    failed = false
    try {
      await setLocale(next)
    } catch {
      failed = true
    } finally {
      busy = false
    }
  }
</script>

<div class="locale">
  <DropdownMenu.Root bind:open dir={localeDir(locale())}>
    <DropdownMenu.Trigger
      class="locale-trigger"
      title={t('language')}
      aria-busy={busy}
      aria-describedby={failed ? errorId : undefined}
      data-testid="locale"
    >
      <svg class="locale-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" aria-hidden="true">
        <path d="M3 6h11M8.5 3.5v2.5M11 6c0 4-3.5 7-7 7M6 10c0 2 2.5 3.5 5.5 3.5" />
        <path d="M13 20l4-9 4 9M14.5 17h5" />
      </svg>
      <span class="sr-only">{t('language')}</span>
      <span lang={locale()}>{localeNames[locale()]}</span>
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content
        bind:ref={content}
        class="select-menu"
        side="top"
        align="start"
        sideOffset={6}
        collisionPadding={8}
        aria-label={t('language')}
        data-testid="locale-menu"
      >
        <DropdownMenu.RadioGroup value={locale()} onValueChange={pick}>
          {#each locales as code (code)}
            <DropdownMenu.RadioItem
              value={code}
              textValue={localeNames[code]}
              disabled={busy}
              class="select-item"
              lang={code}
              data-value={code}
            >
              {#snippet children({ checked })}
                <span>{localeNames[code]}</span>
                {#if checked}
                  <svg class="locale-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
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
  {#if failed}<p id={errorId} class="error" role="alert">{t('languageFailed')}</p>{/if}
</div>

<style>
  .locale p {
    margin: 4px 0 0;
    padding: 0 12px;
    font-size: 12px;
  }
  :global(.locale-trigger) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: transparent;
    color: var(--muted);
  }
  :global(.locale-trigger:hover),
  :global(.locale-trigger[data-state='open']) {
    color: var(--ink);
  }
  :global(.locale-trigger:focus-visible) {
    outline: 2px solid var(--green);
    outline-offset: 1px;
  }
  :global(.locale-trigger[aria-busy='true']) {
    cursor: progress;
    opacity: 0.6;
  }
  .locale-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .locale-check {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
  }
</style>
