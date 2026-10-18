<script lang="ts">
  import { t } from '../i18n/index.svelte'
  import Generator from '../views/vault/Generator.svelte'
  import Icon from './Icon.svelte'
  import Tooltip from './Tooltip.svelte'
  import { buttonClass, cn } from './ui'

  /**
   * A field for a secret being typed: dots until the eye is pressed. It
   * keeps every line of what is typed or pasted, such as a PEM or SSH key
   * or a recovery phrase (a single-line input would drop the line breaks);
   * Enter adds a line only when `multiline`. With `generate`, the password
   * generator fills it.
   */
  let {
    value = $bindable(''),
    placeholder = '',
    generate = false,
    pin = false,
    multiline = false,
    testid
  }: { value?: string; placeholder?: string; generate?: boolean; pin?: boolean; multiline?: boolean; testid?: string } = $props()

  let shown = $state(false)
  const rows = $derived(Math.min(12, Math.max(multiline ? 3 : 1, value.split('\n').length)))

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !multiline && !event.metaKey && !event.ctrlKey) event.preventDefault()
  }
</script>

<div class="relative flex items-start">
  <textarea
    bind:value
    {placeholder}
    {rows}
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
    wrap={multiline ? 'off' : 'soft'}
    class={cn(
      'min-h-8 resize-none py-[5px] font-mono text-[12.5px] leading-[1.55] placeholder:font-sans placeholder:text-[13px]',
      !shown && 'concealed',
      generate ? 'pe-[112px]' : 'pe-9'
    )}
    onkeydown={onKey}
    data-testid={testid}
    data-shown={shown}
  ></textarea>
  <div class="absolute end-1 top-1 flex items-center gap-0.5">
    {#if generate}
      <Generator {pin} onUse={(secret) => ((value = secret), (shown = true))} testid={testid ? `${testid}-generate` : undefined} />
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

<style>
  /* Dots over every line; the value itself keeps its line breaks. */
  .concealed {
    -webkit-text-security: disc;
  }
</style>
