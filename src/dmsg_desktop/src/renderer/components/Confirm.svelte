<script lang="ts">
  import { onDestroy, type Snippet } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { code, errorText } from '../rpc'
  import Notice from './page/Notice.svelte'
  import { toast } from './toast.svelte'
  import Dialog from './Dialog.svelte'
  import type { Glyph } from './glyphs'
  import { buttonClass } from './ui'

  /**
   * A second look before something that cannot be taken back, or reaches
   * far: the person reads what happens, then confirms or cancels. `body`
   * adds what the description cannot say in a line.
   */
  let {
    open = $bindable(false),
    title,
    description,
    confirm,
    icon = 'warning',
    tone = 'danger',
    busy = false,
    focusCancel = false,
    testid,
    confirmTestid,
    onConfirm,
    onReview,
    describeError = errorText,
    body
  }: {
    open?: boolean
    title: string
    description?: string
    confirm: string
    icon?: Glyph
    tone?: 'warn' | 'danger'
    busy?: boolean
    focusCancel?: boolean
    testid?: string
    confirmTestid?: string
    /** Resolve after the write is accepted. Reject with its original error. */
    onConfirm: () => Promise<void>
    /** Read the authoritative state when a submitted operation has no confirmed outcome. */
    onReview: () => unknown
    describeError?: (cause: unknown) => string
    body?: Snippet
  } = $props()

  let cancelButton = $state<HTMLButtonElement>()
  let submitting = $state(false)
  let failure = $state.raw<{ cause: unknown } | null>(null)
  const uncertain = $derived(failure !== null && ['EXECUTION_UNKNOWN', 'PENDING', 'RESULT_EXPIRED'].includes(code(failure.cause)))
  const message = $derived.by(() => {
    if (!failure) return ''
    if (uncertain) return t('operationUnknown')
    if (['FORBIDDEN', 'PERMISSION_DENIED'].includes(code(failure.cause))) return t('confirmForbidden')
    return describeError(failure.cause)
  })
  let visit = 0
  $effect(() => { if (!open) { visit++; submitting = false; failure = null } })
  onDestroy(() => { visit++ })

  function review() {
    open = false
    void Promise.resolve().then(onReview).catch(e => toast(() => describeError(e)))
  }

  async function decide() {
    if (!open || busy || submitting || uncertain) return
    const turn = visit
    submitting = true
    failure = null
    try {
      await onConfirm()
      if (turn === visit) open = false
    } catch (cause) {
      if (turn === visit) failure = { cause }
    } finally { if (turn === visit) submitting = false }
  }
</script>

<Dialog bind:open dismissible={!submitting && !uncertain} {title} {description} {icon} {tone} {testid}
  onOpenAutoFocus={(event) => { if (focusCancel && cancelButton) { event.preventDefault(); cancelButton.focus() } }}>
  {#if body}<fieldset class="min-w-0" disabled={submitting || uncertain}>{@render body()}</fieldset>{/if}
  {#if failure}
    <Notice tone={uncertain ? 'warn' : 'danger'} testid="confirmation-error">{message}</Notice>
  {/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    {#if uncertain}
      <button class={buttonClass('primary')} onclick={review} data-testid="confirmation-review">{t('confirmCheckStatus')}</button>
    {:else}
      <button bind:this={cancelButton} class={buttonClass('ghost')} disabled={submitting} onclick={() => (open = false)}>{t('cancel')}</button>
      <button class={buttonClass(tone === 'danger' ? 'danger' : 'primary')} disabled={busy || submitting} aria-busy={submitting} onclick={decide} data-testid={confirmTestid ?? (testid ? `${testid}-confirm` : undefined)}>{failure ? t('retry') : confirm}</button>
    {/if}
  {/snippet}
</Dialog>
