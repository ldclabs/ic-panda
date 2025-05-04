<script lang="ts">
  import IconCkDOGE from '$lib/components/icons/IconCkDOGE.svelte'
  import IconDOGE from '$lib/components/icons/IconDOGE.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import encodeQR from 'qr'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let dogeAddress: string
  export let principal: string
  export let onFinish: () => void

  let addressInfo = {
    name: 'ckDOGE',
    address: principal
  }

  function onFinishHandler() {
    parent && parent['onClose']()
    onFinish && onFinish()
  }
</script>

<ModalCard {parent}>
  {@const qrcode = encodeQR(addressInfo.address, 'svg', {
    ecc: 'high',
    border: 0
  })}
  <div class="!mt-0 text-center text-xl font-bold"
    >{addressInfo.name == 'DOGE' ? 'Mint' : 'Receive'} ckDOGE</div
  >
  <div class="!mt-6 flex flex-row items-center justify-center gap-2">
    <button
      class="variant-filled btn btn-sm w-32 rounded-md {addressInfo.name ==
      'ckDOGE'
        ? 'bg-gray text-white'
        : 'bg-gray/20 text-black'}"
      on:click={() => {
        addressInfo = {
          name: 'ckDOGE',
          address: principal
        }
      }}
    >
      ckDOGE
    </button>
    <button
      class="variant-filled btn btn-sm w-32 rounded-md {addressInfo.name ==
      'DOGE'
        ? 'bg-gray text-white'
        : 'bg-gray/20 text-black'}"
      on:click={() => {
        addressInfo = {
          name: 'DOGE',
          address: dogeAddress
        }
      }}
    >
      DOGE
    </button>
  </div>
  <div class="!mt-6 text-center">
    <div
      class="relative m-auto size-72 items-center rounded-xl border-2 {addressInfo.name ==
      'ckDOGE'
        ? 'border-[#3b00b9] bg-[#3b00b9]/10'
        : 'border-[#e2cc85] bg-[#e2cc85]/20'} p-2"
    >
      <div class="bg-white">
        {@html qrcode}
      </div>
      <div
        class="absolute top-[calc(50%-24px)] left-[calc(50%-24px)] rounded-md bg-white p-1 *:size-10"
      >
        {#if addressInfo.name === 'ckDOGE'}
          <IconCkDOGE />
        {:else}
          <IconDOGE />
        {/if}
      </div>
    </div>
  </div>
  <div
    class="card bg-gray/5 !mt-6 flex flex-row items-center justify-between p-4"
  >
    <div class="flex flex-row items-center gap-4">
      <div class="flex flex-col">
        <p class="font-bold">{addressInfo.name} Address:</p>
        <p class="min-w-0 text-balance break-words max-sm:max-w-64"
          >{addressInfo.address}</p
        >
      </div>
    </div>
    <div class="space-x-2">
      <TextClipboardButton textValue={addressInfo.address} />
    </div>
  </div>
  <footer class="!mt-6 flex flex-col items-center">
    <button
      class="variant-filled btn bg-gray w-[320px] font-medium outline-none"
      on:click={onFinishHandler}
    >
      Finish
    </button>
  </footer>
</ModalCard>
