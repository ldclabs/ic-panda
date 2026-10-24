<script lang="ts">
  import { Dialog } from 'bits-ui'
  import type { Snippet } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import type { Glyph } from './glyphs'
  import Icon from './Icon.svelte'
  import { iconButtonClass, cn } from './ui'

  /**
   * A modal step on the bits-ui dialog: a flow that should not share the
   * page with a list, such as a new item or settling a conflict. Escape and
   * the close button cancel; `footer` holds the decision.
   */
  let {
    open = $bindable(false),
    title,
    description,
    icon,
    tone = 'forest',
    wide = false,
    dismissible = true,
    testid,
    onOpenAutoFocus,
    children,
    footer
  }: {
    open?: boolean
    title: string
    description?: string
    icon?: Glyph
    tone?: 'forest' | 'warn' | 'danger'
    wide?: boolean
    dismissible?: boolean
    testid?: string
    onOpenAutoFocus?: (event: Event) => void
    children: Snippet
    footer?: Snippet
  } = $props()

  const tones = { forest: 'text-forest', warn: 'text-warn', danger: 'text-danger' }
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay
      class="fixed inset-0 z-50 bg-ink/25 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:animate-in data-[state=open]:fade-in-0"
    />
    <Dialog.Content
      {onOpenAutoFocus}
      escapeKeydownBehavior={dismissible ? 'close' : 'ignore'}
      interactOutsideBehavior={dismissible ? 'close' : 'ignore'}
      class={cn(
        'fixed top-1/2 left-1/2 z-50 flex max-h-[calc(100vh-48px)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl border border-line bg-surface text-ink shadow-pop outline-none duration-200 ease-spring-soft data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-98 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-98',
        wide ? 'w-[min(760px,calc(100vw-32px))]' : 'w-[min(520px,calc(100vw-32px))]'
      )}
      data-testid={testid}
    >
      <header class="flex items-start gap-3 px-5 pt-4 pb-2">
        {#if icon}
          <span class={cn('grid h-9 w-5 shrink-0 place-items-center [&_svg]:size-5', tones[tone])}><Icon name={icon} /></span>
        {/if}
        <div class="min-w-0 flex-1">
          <Dialog.Title class="pt-2 text-[15px] leading-5 font-semibold [overflow-wrap:anywhere]">{title}</Dialog.Title>
          {#if description}<Dialog.Description class="mt-0.5 text-[12.5px] leading-normal text-muted">{description}</Dialog.Description>{/if}
        </div>
        <Dialog.Close class={iconButtonClass('ghost', 'lg')} disabled={!dismissible} aria-label={t('close')} data-testid="dialog-close">
          <Icon name="close" />
        </Dialog.Close>
      </header>
      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {@render children()}
      </div>
      {#if footer}
        <footer class="flex flex-wrap items-center gap-2 border-t border-line px-5 py-3">
          {@render footer()}
        </footer>
      {/if}
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
