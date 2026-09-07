<script lang="ts">
  import { clipboard } from '$lib/actions/clipboard'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import HoverPopup from '$lib/components/ui/HoverPopup.svelte'

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
  <HoverPopup>
    {#snippet trigger(props)}
      <span {...props} class="text-neutral-500 {copiedClass}">{textName}</span>
    {/snippet}
    {#snippet content()}
      <p class="text-pretty break-all">{textValue}</p>
    {/snippet}
  </HoverPopup>
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
</div>
