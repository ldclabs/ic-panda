<script lang="ts">
  import { t } from '$lib/i18n'
  /**
   * Replaces Skeleton's `FileButton`: a hidden file input driven by a visible
   * button, so the trigger can be styled freely.
   */
  interface Props {
    class?: string
    name: string
    button?: string
    width?: string
    accept?: string
    multiple?: boolean
    disabled?: boolean
    files?: FileList | undefined
    onchange?: (event: Event) => void
    children?: import('svelte').Snippet
  }

  let {
    class: selfClass = '',
    name,
    button = 'btn variant-filled',
    width = '',
    accept,
    multiple = false,
    disabled = false,
    files = $bindable(undefined),
    onchange,
    children
  }: Props = $props()

  let fileInput: HTMLInputElement | undefined = $state()
</script>

<div class="file-button {selfClass}" data-testid="file-button">
  <!-- NOTE: Don't use `hidden` as it prevents `required` from operating -->
  <div class="h-0 w-0 overflow-hidden">
    <input
      type="file"
      bind:this={fileInput}
      bind:files
      {name}
      {accept}
      {multiple}
      {disabled}
      {onchange}
    />
  </div>
  <button
    type="button"
    class="file-button-btn {button} {width}"
    {disabled}
    onclick={() => fileInput?.click()}
  >
    {#if children}{@render children()}{:else}{$t('Select a File')}{/if}
  </button>
</div>
