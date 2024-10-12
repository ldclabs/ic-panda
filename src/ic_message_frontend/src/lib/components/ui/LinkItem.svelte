<script lang="ts">
  import { type Link } from '$lib/canisters/messageprofile'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import IconLink from '$lib/components/icons/IconLink.svelte'
  import { clipboard } from '@skeletonlabs/skeleton'

  export let link: Link

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = '!text-primary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 3000)
  }
</script>

{#if link.uri.startsWith('http')}
  <a
    type="button"
    href={link.uri}
    target="_blank"
    rel="noopener noreferrer"
    class="bg-surface-hover-token bg-surface-50-900-token flex w-full flex-row items-center justify-center gap-2 rounded-lg px-2 py-4"
  >
    <span>{link.title}</span>
    <span class="text-surface-500 *:size-5"><IconLink /></span>
  </a>
{:else}
  <button
    class="bg-surface-hover-token bg-surface-50-900-token flex w-full flex-row items-center justify-center gap-2 rounded-lg px-2 py-4"
    use:clipboard={link.uri}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    <span>{link.title}</span>
    <span class={copiedClass}>{link.uri}</span>
    <span class="text-surface-500 *:size-5"
      >{#if copiedClass != ''}
        <IconCheckbox />
      {:else}
        <IconCopy />
      {/if}</span
    >
  </button>
{/if}
