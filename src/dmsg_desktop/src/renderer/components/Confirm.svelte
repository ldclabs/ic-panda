<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '../i18n/index.svelte'
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
    testid,
    onConfirm,
    body
  }: {
    open?: boolean
    title: string
    description?: string
    confirm: string
    icon?: Glyph
    tone?: 'warn' | 'danger'
    busy?: boolean
    testid?: string
    onConfirm: () => unknown
    body?: Snippet
  } = $props()

  async function decide() {
    await onConfirm()
    open = false
  }
</script>

<Dialog bind:open {title} {description} {icon} {tone} {testid}>
  {#if body}{@render body()}{/if}
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button>
    <button class={buttonClass(tone === 'danger' ? 'danger' : 'primary')} disabled={busy} onclick={decide} data-testid={testid ? `${testid}-confirm` : undefined}>{confirm}</button>
  {/snippet}
</Dialog>
