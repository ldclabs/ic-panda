<script lang="ts">
  import { type Link } from '$lib/canisters/messageprofile'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import IconLink from '$lib/components/icons/IconLink.svelte'
  import IconQrCode from '$lib/components/icons/IconQrCode.svelte'
  import { clipboard } from '@skeletonlabs/skeleton'

  export let link: Link
  export let onQrHandler:
    | ((qrTitle: string, qrValue: string, qrLogo?: string) => void)
    | null = null

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
    class="bg-surface-hover-token bg-surface-50-900-token flex w-full flex-row items-center justify-center gap-2 text-pretty break-all rounded-lg px-2 py-4"
  >
    <span>{link.title}</span>
    <span class="text-surface-500 *:size-5"><IconLink /></span>
    {#if onQrHandler}
      <button
        class="flex flex-row items-center gap-2"
        on:click|stopPropagation|preventDefault={() =>
          onQrHandler(link.title, link.uri)}
      >
        <span class="text-surface-500 *:size-5"><IconQrCode /></span>
      </button>
    {/if}
  </a>
{:else}
  <button
    class="bg-surface-hover-token bg-surface-50-900-token flex w-full flex-row items-center justify-center gap-2 text-pretty break-all rounded-lg px-2 py-4"
    use:clipboard={link.uri}
    on:click={onCopyHandler}
    disabled={copiedClass != ''}
  >
    <div>{link.title}<span class="ml-2 {copiedClass}">{link.uri}</span></div>
    <span class="text-surface-500 *:size-5"
      >{#if copiedClass != ''}
        <IconCheckbox />
      {:else}
        <IconCopy />
      {/if}</span
    >
    {#if onQrHandler}
      <a
        class="flex flex-row items-center gap-2"
        type="button"
        role="button"
        href="/"
        tabindex="0"
        on:click|stopPropagation|preventDefault={() =>
          onQrHandler(link.title, link.uri)}
      >
        <span class="text-surface-500 *:size-5"><IconQrCode /></span>
      </a>
    {/if}
  </button>
{/if}
