<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements'
  import { t } from '../i18n/index.svelte'
  import { copyText } from './clipboard'
  import type { Glyph } from './glyphs'
  import Icon from './Icon.svelte'
  import { pop } from './motion'

  /**
   * Copies text the window already shows, such as a link or a token. The
   * button's icon turns to a check for 1.2s in place of a notice, and a
   * status says it for a screen reader; a failure still shows a notice.
   * Other attributes (a tooltip's props, `aria-label`, `disabled`) go to the
   * button. Secrets are copied through the main process instead.
   */
  let {
    value,
    label,
    icon = 'copy',
    testid,
    ...rest
  }: Omit<HTMLButtonAttributes, 'value'> & {
    value: string | (() => string)
    label?: string
    icon?: Glyph
    testid?: string
  } = $props()

  let copied = $state(false)
  let timer: ReturnType<typeof setTimeout> | undefined

  async function copy() {
    if (!(await copyText(typeof value === 'function' ? value() : value, null))) return
    copied = true
    clearTimeout(timer)
    timer = setTimeout(() => (copied = false), 1200)
  }

  $effect(() => () => clearTimeout(timer))
</script>

<button {...rest} data-testid={testid} data-copied={copied || undefined} onclick={copy}>
  {#key copied}<span class="inline-flex shrink-0" in:pop><Icon name={copied ? 'check' : icon} /></span>{/key}{label ?? ''}
</button>
<span class="sr-only" role="status">{copied ? t('copiedText') : ''}</span>
