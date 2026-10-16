<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import Icon from './Icon.svelte'
  import Tooltip from './Tooltip.svelte'
  import { buttonClass } from './ui'

  /**
   * A field for a secret being typed: hidden until the eye is pressed, and
   * with `generate`, a button that fills in a random 20-character one.
   */
  let {
    value = $bindable(''),
    placeholder = '',
    generate = false,
    testid
  }: { value?: string; placeholder?: string; generate?: boolean; testid?: string } = $props()

  let shown = $state(false)

  const ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789-_.!@#%'

  function fill() {
    // 64 symbols, so a byte modulo 64 is uniform.
    const bytes = crypto.getRandomValues(new Uint8Array(20))
    value = Array.from(bytes, (byte) => ALPHABET[byte % ALPHABET.length]).join('')
    shown = true
  }
</script>

<div class="relative flex items-center">
  <input
    type={shown ? 'text' : 'password'}
    bind:value
    {placeholder}
    autocomplete="off"
    spellcheck="false"
    class="font-mono text-[12.5px] placeholder:font-sans placeholder:text-[13px] {generate ? 'pe-[104px]' : 'pe-9'}"
    data-testid={testid}
  />
  <div class="absolute end-1 flex items-center gap-0.5">
    {#if generate}
      <button type="button" class={buttonClass('ghost', 'xs', 'text-forest')} onclick={fill}>{t('generate')}</button>
    {/if}
    <Tooltip content={shown ? t('hideSecret') : t('showTyped')}>
      {#snippet children(props)}
        <button {...props} type="button" class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={shown ? t('hideSecret') : t('showTyped')} onclick={() => (shown = !shown)}>
          <Icon name={shown ? 'eye-off' : 'eye'} />
        </button>
      {/snippet}
    </Tooltip>
  </div>
</div>
