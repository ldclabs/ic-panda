<script lang="ts">
  import { clipboard, popup } from '@skeletonlabs/skeleton'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'

  export let textName: string
  export let textValue: string

  const textHover: PopupSettings = {
    event: 'hover',
    target: 'textHover-' + textName
  }

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = 'text-secondary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<div class="flex flex-row gap-2">
  <span class="font-medium {copiedClass}" use:popup={textHover}>
    {textName}
  </span>
  <button
    class={copiedClass}
    use:clipboard={textValue}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    <IconCopy />
  </button>
  <div
    class="card bg-surface-800 px-2 py-1 text-white"
    data-popup="textHover-{textName}"
  >
    <p>{textValue}</p>
    <div class="arrow bg-surface-800" />
  </div>
</div>
