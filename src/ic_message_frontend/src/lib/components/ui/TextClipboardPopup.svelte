<script lang="ts">
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import { clipboard, popup } from '@skeletonlabs/skeleton'

  interface Props {
    class?: string
    textLable: string
    textName: string
    textValue: string
  }

  let {
    class: selfClass = 'align-middle',
    textLable,
    textName,
    textValue
  }: Props = $props()

  const textHover: PopupSettings = {
    event: 'hover',
    target: 'textHover-' + (textName || textValue),
    placement: 'top'
  }

  let copiedClass = $state('')

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
  <span class="text-neutral-500 {copiedClass}" use:popup={textHover}>
    {textName}
  </span>
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
  <div
    class="card max-w-80 bg-surface-800 px-2 py-1 text-white"
    data-popup={textHover.target}
  >
    <p class="text-pretty break-all">{textValue}</p>
    <div class="arrow bg-surface-800"></div>
  </div>
</div>
