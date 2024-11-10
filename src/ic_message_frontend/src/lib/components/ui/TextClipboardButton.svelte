<script lang="ts">
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import { clipboard } from '@skeletonlabs/skeleton'

  interface Props {
    textValue: string
  }

  let { textValue }: Props = $props()

  let copiedClass = $state('')

  function onCopyHandler(): void {
    copiedClass = '!text-panda'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<button
  class="{copiedClass} float-right mt-[3px] *:size-5"
  use:clipboard={textValue}
  onclick={onCopyHandler}
  disabled={copiedClass != ''}
>
  {#if copiedClass != ''}
    <IconCheckbox />
  {:else}
    <IconCopy />
  {/if}
</button>
