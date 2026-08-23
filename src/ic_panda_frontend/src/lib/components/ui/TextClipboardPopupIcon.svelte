<script lang="ts">
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  let selfClass: string = 'align-middle'

  export { selfClass as class }
  export let textName: string
  export let textValue: string

  let copiedClass = ''

  async function onCopyHandler(): Promise<void> {
    await navigator.clipboard.writeText(textValue)
    copiedClass = '!text-panda'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<div class={selfClass}>
  <button
    class="{copiedClass} float-right mt-[3px] *:size-5"
    title={textName}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    {#if copiedClass != ''}
      <IconCheckbox />
    {:else}
      <IconCopy />
    {/if}
  </button>
</div>
