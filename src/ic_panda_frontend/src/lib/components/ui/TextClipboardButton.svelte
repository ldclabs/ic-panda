<script lang="ts">
  import { t } from '$lib/i18n'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  export let textValue: string

  let copiedClass = ''

  async function onCopyHandler(): Promise<void> {
    // Rejects when the document isn't focused, and is missing outright on a
    // non-secure origin. Don't claim success we didn't get.
    try {
      await navigator.clipboard.writeText(textValue)
    } catch (err) {
      console.error('Copy to clipboard failed:', err)
      return
    }
    copiedClass = '!text-panda'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<button
  aria-label={$t('Copy to clipboard')}
  class="{copiedClass} float-right mt-[3px] *:size-5"
  on:click={onCopyHandler}
  disabled={copiedClass != ''}
>
  {#if copiedClass != ''}
    <IconCheckbox />
  {:else}
    <IconCopy />
  {/if}
</button>
