<script lang="ts">
  import { t } from '$lib/i18n'
  import { clipboard } from '$lib/actions/clipboard'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import HoverPopup from '$lib/components/ui/HoverPopup.svelte'

  interface Props {
    class?: string
    textName: string
    textValue: string
  }

  let {
    class: selfClass = 'align-middle',
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
  <HoverPopup>
    {#snippet trigger(props)}
      <button
        aria-label={$t('Copy to clipboard')}
        {...props}
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
    {/snippet}
    {#snippet content()}
      <p class="text-pretty break-all">{textName}</p>
    {/snippet}
  </HoverPopup>
</div>
