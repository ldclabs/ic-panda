<script lang="ts">
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import { clipboard, popup } from '@skeletonlabs/skeleton'

  let selfClass: string = 'align-middle'

  export { selfClass as class }
  export let textName: string
  export let textValue: string

  const textHover: PopupSettings = {
    event: 'hover',
    target: 'iconHover-' + (textName || textValue),
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

<div class={selfClass}>
  <button
    class="{copiedClass} float-right mt-[3px] *:size-4"
    use:popup={textHover}
    use:clipboard={textValue}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    <IconCopy />
  </button>
  <div
    class="card max-w-80 bg-surface-800 px-2 py-1 text-white"
    data-popup={textHover.target}
  >
    <p class="text-pretty break-words">{textName}</p>
    <div class="arrow bg-surface-800" />
  </div>
</div>
