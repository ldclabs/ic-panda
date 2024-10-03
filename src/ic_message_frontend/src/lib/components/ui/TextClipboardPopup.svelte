<script lang="ts">
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import { clipboard, popup } from '@skeletonlabs/skeleton'

  let selfClass: string = 'align-middle'

  export { selfClass as class }
  export let textLable: string
  export let textName: string
  export let textValue: string

  const textHover: PopupSettings = {
    event: 'hover',
    target: 'textHover-' + (textName || textValue),
    placement: 'top'
  }

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = '!text-panda'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }
</script>

<div class={selfClass}>
  {#if textLable != ''}
    <span class="mr-2 font-medium">{textLable}</span>
  {/if}
  <span class="text-neutral-600 {copiedClass}" use:popup={textHover}>
    {textName}
  </span>
  <button
    class="{copiedClass} float-right mt-[3px] *:size-5"
    use:clipboard={textValue}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    {#if copiedClass != ''}
      <IconCheckbox />
    {:else}
      <IconCopy />
    {/if}
  </button>
  <div
    class="card max-w-80 bg-surface-800 px-2 py-1 text-white"
    data-popup={textHover.target}
  >
    <p class="text-balance break-words">{textValue}</p>
    <div class="arrow bg-surface-800" />
  </div>
</div>
