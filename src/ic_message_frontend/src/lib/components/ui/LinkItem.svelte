<script lang="ts">
  import { type Link } from '$lib/canisters/messageprofile'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCopy from '$lib/components/icons/IconCopy.svelte'
  import IconLink from '$lib/components/icons/IconLink.svelte'
  import IconQrCode from '$lib/components/icons/IconQrCode.svelte'
  import { clipboard } from '@skeletonlabs/skeleton'

  type OnQrHandler = (qrTitle: string, qrValue: string, qrLogo?: string) => void

  interface Props {
    link: Link
    onQrHandler?: OnQrHandler | null
  }

  let { link, onQrHandler = null }: Props = $props()

  let copiedClass = $state('')

  function onCopyHandler(): void {
    copiedClass = '!text-primary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 3000)
  }
</script>

{#snippet qrButton(link: Link, onQrHandler: OnQrHandler)}
  <button
    class="flex flex-row items-center gap-2"
    type="button"
    onclick={(ev) => {
      ev.preventDefault()
      ev.stopPropagation()
      onQrHandler(link.title, link.uri)
    }}
  >
    <span class="text-surface-500 *:size-5"><IconQrCode /></span>
  </button>
{/snippet}

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
      {@render qrButton(link, onQrHandler)}
    {/if}
  </a>
{:else}
  <a
    type="button"
    href="/"
    class="bg-surface-hover-token bg-surface-50-900-token flex w-full flex-row items-center justify-center gap-2 text-pretty break-all rounded-lg px-2 py-4"
    use:clipboard={link.uri}
    onclick={(ev) => {
      ev.preventDefault()
      ev.stopPropagation()
      onCopyHandler()
    }}
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
      {@render qrButton(link, onQrHandler)}
    {/if}
  </a>
{/if}
