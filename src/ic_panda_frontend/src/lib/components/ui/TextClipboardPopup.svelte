<script lang="ts">
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import { clipboard, popup } from '@skeletonlabs/skeleton'

  export let textLable: string
  export let textName: string
  export let textValue: string

  const textHover: PopupSettings = {
    event: 'hover',
    target: 'textHover-' + textName,
    placement: 'top'
  }

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = '!text-secondary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<div class="align-middle">
  <span class="mr-2 font-medium">{textLable}</span>
  <span class="text-gray/40 {copiedClass}" use:popup={textHover}>
    {textName}
  </span>
  <button
    class="{copiedClass} float-right mt-[3px] *:size-4"
    use:clipboard={textValue}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    <IconCopy />
  </button>
  <div
    class="card max-w-80 bg-surface-800 px-2 py-1 text-white"
    data-popup="textHover-{textName}"
  >
    <p class="text-pretty break-words">{textValue}</p>
    <div class="arrow bg-surface-800" />
  </div>
</div>
