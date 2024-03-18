<script lang="ts">
  import { clipboard } from '@skeletonlabs/skeleton'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'

  export let textValue: string

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = '!text-secondary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<button
  class="{copiedClass} float-right mt-[3px] *:size-4"
  use:clipboard={textValue}
  on:click={onCopyHandler}
  disabled={copiedClass != ''}
>
  <IconCopy />
</button>
