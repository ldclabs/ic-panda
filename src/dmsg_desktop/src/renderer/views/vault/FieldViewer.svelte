<script lang="ts">
  import { tick, type Snippet } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import { cn } from '../../components/ui'

  let { open = $bindable(false), name, context, value, mono = false, ltr = mono, trigger, footer }: {
    open?: boolean
    name: string
    context?: string
    value: string | null
    mono?: boolean
    ltr?: boolean
    trigger?: HTMLElement | null
    footer?: Snippet
  } = $props()

  function restoreFocus(event: Event) {
    event.preventDefault()
    // Closing cancels pending reveals and re-enables their field actions.
    void tick().then(() => { if (trigger?.isConnected) trigger.focus() })
  }
</script>

<Dialog bind:open title={name} description={context} wide testid="vault-field-viewer"
  onCloseAutoFocus={restoreFocus} {footer}>
  <div class={cn('text-[28px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere] select-text', mono && 'font-mono', ltr && 'text-left', value === null && 'tracking-[0.18em] text-faint')}
    dir={ltr ? 'ltr' : 'auto'} data-testid="vault-large-value" data-shown={value !== null}>{value ?? '••••••••••••'}</div>
</Dialog>
