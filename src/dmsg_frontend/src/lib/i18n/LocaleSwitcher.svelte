<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import {
    locale,
    localeDir,
    locales,
    localeNames,
    setLocale,
    t,
    type Locale
  } from './index'

  // Same bare trigger, icons and menu surface as TokenList's LocaleSwitcher;
  // keep this app's asynchronous catalog loading and persistence API.
  let {
    class: className = '',
    compact = false
  }: { class?: string; compact?: boolean } = $props()

  let open = $state(false)
  let content = $state<HTMLElement | null>(null)
  let busy = $state(false)
  let error = $state('')
  const errorId = $props.id()

  // Arrow-key navigation belongs to the menu content in bits-ui.
  $effect(() => {
    if (open) content?.focus()
  })

  async function pick(code: string) {
    if (busy) return
    busy = true
    error = ''
    try {
      await setLocale(code as Locale)
    } catch {
      error = 'Could not load this language. Please try again.'
    } finally {
      busy = false
    }
  }
</script>

<div class="locale-picker">
  <DropdownMenu.Root dir={localeDir($locale)} bind:open>
    <DropdownMenu.Trigger
      class={['locale-picker-trigger', className]}
      title={$t('Language')}
      aria-busy={busy}
      aria-describedby={error ? errorId : undefined}
    >
      <span class="sr-only">{$t('Language')}</span>
      <svg
        class="locale-picker-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linecap="square"
        aria-hidden="true"
      >
        <path
          d="M3 6h11M8.5 3.5v2.5M11 6c0 4-3.5 7-7 7M6 10c0 2 2.5 3.5 5.5 3.5"
        />
        <path d="M13 20l4-9 4 9M14.5 17h5" />
      </svg>
      <!-- The current language stays in the accessible name in compact mode. -->
      <span class={compact ? 'sr-only' : 'locale-picker-name'} lang={$locale}>
        {localeNames[$locale]}
      </span>
      <svg
        class="locale-picker-chevron"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.75"
        stroke-linecap="square"
        stroke-linejoin="miter"
        aria-hidden="true"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </DropdownMenu.Trigger>

    <DropdownMenu.Portal>
      <DropdownMenu.Content
        bind:ref={content}
        class="locale-picker-menu"
        align="end"
        sideOffset={8}
        collisionPadding={8}
        aria-label={$t('Language')}
      >
        <DropdownMenu.RadioGroup value={$locale} onValueChange={pick}>
          {#each locales as code (code)}
            <DropdownMenu.RadioItem
              value={code}
              textValue={localeNames[code]}
              disabled={busy}
              class="locale-picker-item"
              lang={code}
            >
              <span>{localeNames[code]}</span>
              {#if code === $locale}
                <svg
                  class="locale-picker-check"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.75"
                  stroke-linecap="square"
                  stroke-linejoin="miter"
                  aria-hidden="true"
                >
                  <path d="M4 12.5 9.5 18 20 6" />
                </svg>
              {/if}
            </DropdownMenu.RadioItem>
          {/each}
        </DropdownMenu.RadioGroup>
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>

  {#if error}
    <span id={errorId} class="locale-picker-error" role="alert"
      >{$t(error)}</span
    >
  {/if}
</div>

<style>
  .locale-picker {
    position: relative;
    flex-shrink: 0;
  }

  :global(.locale-picker-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 36px;
    padding: 4px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: #4c5460;
    font: inherit;
    cursor: pointer;
    transition: color 150ms cubic-bezier(0.2, 0.6, 0.2, 1);
  }
  :global(.locale-picker-trigger:hover),
  :global(.locale-picker-trigger[data-state='open']) {
    color: #13181e;
  }
  :global(.locale-picker-trigger:focus-visible) {
    outline: 2px solid currentColor;
    outline-offset: 4px;
  }
  :global(.locale-picker-trigger[aria-busy='true']) {
    cursor: progress;
    opacity: 0.6;
  }

  .locale-picker-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
  }
  .locale-picker-chevron {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    opacity: 0.6;
    transition: transform 150ms cubic-bezier(0.2, 0.6, 0.2, 1);
  }
  :global(.locale-picker-trigger[data-state='open']) .locale-picker-chevron {
    transform: rotate(180deg);
  }
  .locale-picker-name {
    font-size: 14px;
    line-height: 20px;
    font-weight: 500;
  }

  :global(.locale-picker-menu) {
    z-index: 10002;
    min-width: 144px;
    max-height: min(60vh, 420px);
    overflow-y: auto;
    padding: 4px;
    border: 1px solid #c5cedb;
    border-radius: 8px;
    background: #fff;
    color: #13181e;
    font-family: var(--locale-font, Inter, Archivo, ui-sans-serif, sans-serif);
    box-shadow: 0 8px 32px rgb(9 11 15 / 16%);
    scrollbar-width: thin;
    animation: locale-picker-enter 140ms cubic-bezier(0.2, 0.6, 0.2, 1);
  }
  :global(.locale-picker-item) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 6px 10px;
    border-radius: 2px;
    outline: none;
    color: #4c5460;
    font-size: 14px;
    line-height: 20px;
    white-space: nowrap;
    cursor: pointer;
    user-select: none;
    transition: background-color 150ms;
  }
  :global(.locale-picker-item[data-state='checked']) {
    color: #13181e;
    font-weight: 500;
  }
  :global(.locale-picker-item[data-highlighted]) {
    background: #e7ecf3;
  }
  :global(.locale-picker-item[data-disabled]) {
    cursor: progress;
    opacity: 0.6;
  }
  .locale-picker-check {
    width: 14px;
    height: 14px;
    flex-shrink: 0;
    color: oklch(0.5 0.1 155);
  }

  .locale-picker-error {
    position: absolute;
    inset-inline-end: 0;
    top: 100%;
    z-index: 10002;
    width: max-content;
    max-width: min(240px, calc(100vw - 24px));
    padding: 8px 12px;
    border: 1px solid #c5cedb;
    border-radius: 8px;
    background: #fff;
    color: #991b1b;
    font-size: 13px;
    line-height: 1.5;
    box-shadow: 0 8px 32px rgb(9 11 15 / 16%);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  @media (max-width: 639px) {
    .locale-picker-name {
      position: absolute;
      width: 1px;
      height: 1px;
      padding: 0;
      margin: -1px;
      overflow: hidden;
      clip: rect(0, 0, 0, 0);
      white-space: nowrap;
      border: 0;
    }
  }
  @keyframes -global-locale-picker-enter {
    from {
      opacity: 0;
      transform: translateY(-2px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.locale-picker-menu) {
      animation: none;
    }
    :global(.locale-picker-trigger),
    :global(.locale-picker-item),
    .locale-picker-chevron {
      transition: none;
    }
  }
</style>
