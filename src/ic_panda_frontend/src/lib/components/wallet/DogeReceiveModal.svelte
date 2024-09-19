<script lang="ts">
  import IconDOGE from '$lib/components/icons/IconDOGE.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import encodeQR from '@paulmillr/qr'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let dogeAddress: string
  export let onFinish: () => void

  function onFinishHandler() {
    parent && parent['onClose']()
    onFinish && onFinish()
  }
</script>

<ModalCard {parent}>
  {@const qrcode = encodeQR(dogeAddress, 'svg', { ecc: 'high', border: 1 })}
  <div class="!mt-0 text-center text-xl font-bold">Receive DOGE</div>
  <div class="text-center">
    <div
      class="relative m-auto size-72 items-center rounded-xl border-2 border-[#e2cc85] bg-[#e2cc85]/20 p-2"
    >
      <div class="bg-white">
        {@html qrcode}
      </div>
      <div
        class="absolute left-[calc(50%-24px)] top-[calc(50%-24px)] rounded-md bg-white p-1 *:size-10"
        ><IconDOGE /></div
      >
    </div>
  </div>
  <div
    class="card !mt-6 flex flex-row items-center justify-between bg-gray/5 p-4"
  >
    <div class="flex flex-row items-center gap-4">
      <div class="flex flex-col">
        <p class="font-bold">DOGE Address:</p>
        <p class="min-w-0 text-balance break-words max-sm:max-w-64"
          >{dogeAddress}</p
        >
      </div>
    </div>
    <div class="space-x-2">
      <TextClipboardButton textValue={dogeAddress} />
    </div>
  </div>
  <footer class="!mt-6 flex flex-col items-center">
    <button
      class="variant-filled btn w-[320px] bg-gray font-medium outline-none"
      on:click={onFinishHandler}
    >
      Finish
    </button>
  </footer>
</ModalCard>
