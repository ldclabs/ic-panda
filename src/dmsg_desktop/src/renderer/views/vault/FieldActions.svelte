<script lang="ts">
  import { ContextMenu, DropdownMenu } from 'bits-ui'
  import { onDestroy, type Snippet } from 'svelte'
  import { localeDir } from '../../../shared/locales'
  import Icon from '../../components/Icon.svelte'
  import type { Glyph } from '../../components/glyphs'
  import Tooltip from '../../components/Tooltip.svelte'
  import { cn, iconButtonClass, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'

  let {
    name, children, beforeActions, copy, copyLabel, copyTestid = 'copy-field-value',
    view, openWebsite, reference, referenceDisabled = false, referenceHint, busy = false
  }: {
    name: string
    children: Snippet
    beforeActions?: Snippet
    copy?: () => Promise<boolean>
    copyLabel?: string
    copyTestid?: string
    view?: (trigger: HTMLElement | null) => void
    openWebsite?: () => Promise<void>
    reference?: () => Promise<boolean>
    referenceDisabled?: boolean
    referenceHint?: string
    busy?: boolean
  } = $props()

  type Action = { kind: 'copy' | 'view' | 'website' | 'reference'; label: string; icon: Glyph; disabled?: boolean; hint?: string }
  let menuTrigger = $state<HTMLButtonElement | null>(null)
  let selectingText = $state(false)
  let working = $state<'copy' | 'reference' | null>(null)
  let feedback = $state<'copy' | 'reference' | null>(null)
  let feedbackTimer: ReturnType<typeof setTimeout> | undefined
  let openingViewer = false
  let alive = true
  const actions = $derived([
    ...(copy ? [{ kind: 'copy', label: copyLabel ?? t('copy'), icon: 'copy' } as Action] : []),
    ...(openWebsite ? [{ kind: 'website', label: t('openWebsite'), icon: 'external' } as Action] : []),
    ...(view ? [{ kind: 'view', label: t('viewField'), icon: 'search' } as Action] : []),
    ...(reference ? [{ kind: 'reference', label: t('copyFieldReference'), icon: 'link', disabled: referenceDisabled, hint: referenceHint ?? t('fieldReferenceHint') } as Action] : [])
  ])

  async function perform(kind: Action['kind']) {
    if (busy || working) return
    if (kind === 'view') { openingViewer = true; view?.(menuTrigger); return }
    if (kind === 'website') { await openWebsite?.(); return }
    if (kind === 'reference' && referenceDisabled) return
    working = kind
    try {
      const copied = await (kind === 'copy' ? copy?.() : reference?.())
      if (alive && copied) {
        feedback = kind
        clearTimeout(feedbackTimer)
        feedbackTimer = setTimeout(() => (feedback = null), 2000)
      }
    } finally { if (alive) working = null }
  }

  function closeFocus(event: Event) {
    if (openingViewer) { event.preventDefault(); openingViewer = false }
  }

  onDestroy(() => { alive = false; clearTimeout(feedbackTimer) })
</script>

{#snippet actionLabel(action: Action)}
  <span class="flex min-w-0 items-start gap-2">
    <Icon name={action.icon} class="mt-0.5 shrink-0 text-faint" />
    <span class="min-w-0">
      <span class="block">{action.label}</span>
      {#if action.hint}<span class="mt-1 block max-w-64 text-[11.5px] leading-normal text-muted">{action.hint}</span>{/if}
    </span>
  </span>
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="contents" oncontextmenucapture={() => (selectingText = !!window.getSelection()?.toString())}>
  <ContextMenu.Root dir={localeDir(locale())}>
    <ContextMenu.Trigger disabled={selectingText || !actions.length} class="group flex min-w-0 flex-1 items-start gap-1.5">
      {@render children()}
      <div class="flex shrink-0 items-start gap-1">
        {#if beforeActions}{@render beforeActions()}{/if}
        {#if copy}
          <Tooltip content={feedback === 'copy' ? t('fieldValueCopied') : copyLabel ?? t('copy')}>
            {#snippet children(props)}
              <button {...props} class={iconButtonClass('ghost', 'md', cn(feedback === 'copy' && 'text-forest'))}
                aria-label={`${copyLabel ?? t('copy')} · ${name}`} aria-busy={working === 'copy'} disabled={busy || working !== null}
                onclick={() => perform('copy')} data-testid={copyTestid} data-copied={feedback === 'copy'}>
                <Icon name={working === 'copy' ? 'sync' : feedback === 'copy' ? 'check' : 'copy'} class={working === 'copy' ? 'animate-spin motion-reduce:animate-none' : ''} />
              </button>
            {/snippet}
          </Tooltip>
        {/if}
        {#if actions.length}
          <DropdownMenu.Root dir={localeDir(locale())}>
            <Tooltip content={feedback === 'reference' ? t('fieldReferenceCopied') : t('fieldActions', { field: name })}>
              {#snippet children(props)}
                <DropdownMenu.Trigger {...props} bind:ref={menuTrigger} class={iconButtonClass('ghost', 'md', cn(feedback === 'reference' && 'text-forest'))}
                  aria-label={t('fieldActions', { field: name })} aria-busy={working === 'reference'} disabled={busy || working !== null} data-testid="vault-field-menu" data-copied={feedback === 'reference'}>
                  <Icon name={working === 'reference' ? 'sync' : feedback === 'reference' ? 'check' : 'more'} class={working === 'reference' ? 'animate-spin motion-reduce:animate-none' : ''} />
                </DropdownMenu.Trigger>
              {/snippet}
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={6} onCloseAutoFocus={closeFocus} class={cn(menuContentClass, 'min-w-48')} data-testid="vault-field-menu-content">
                {#each actions as action (action.kind)}
                  {#if action.kind === 'reference'}<DropdownMenu.Separator class="my-1 h-px bg-line" />{/if}
                  <DropdownMenu.Item class={menuItemClass} disabled={action.disabled || busy || working !== null} onSelect={() => void perform(action.kind)}
                    data-testid={action.kind === 'reference' ? 'vault-copy-field-reference' : `vault-field-${action.kind}`}>
                    {@render actionLabel(action)}
                  </DropdownMenu.Item>
                {/each}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
    </ContextMenu.Trigger>
    <ContextMenu.Portal>
      <ContextMenu.Content onCloseAutoFocus={closeFocus} class={cn(menuContentClass, 'min-w-48')} data-testid="vault-field-context-menu">
        {#each actions as action (action.kind)}
          {#if action.kind === 'reference'}<ContextMenu.Separator class="my-1 h-px bg-line" />{/if}
          <ContextMenu.Item class={menuItemClass} disabled={action.disabled || busy || working !== null} onSelect={() => void perform(action.kind)}
            data-testid={action.kind === 'reference' ? 'vault-copy-field-reference' : `vault-field-${action.kind}`}>
            {@render actionLabel(action)}
          </ContextMenu.Item>
        {/each}
      </ContextMenu.Content>
    </ContextMenu.Portal>
  </ContextMenu.Root>
</div>
