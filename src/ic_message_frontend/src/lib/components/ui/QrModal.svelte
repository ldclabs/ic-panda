<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import encodeQR from '@paulmillr/qr'
  import { Avatar } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  interface Props {
    parent: SvelteComponent
    qrTitle?: string
    qrValue?: string
    qrLogo?: string
  }

  let {
    parent,
    qrTitle = 'ICPanda Message',
    qrValue = 'https://dmsg.net',
    qrLogo = ''
  }: Props = $props()
</script>

<ModalCard {parent}>
  {@const qrcode = encodeQR(qrValue, 'svg', {
    ecc: 'high',
    border: 0
  })}
  <div class="!mt-0 truncate text-center text-xl font-bold">{qrTitle}</div>
  <div
    class="relative m-auto !mt-4 items-center rounded-xl border-2 border-primary-500/50 bg-primary-500/10 p-2"
  >
    <div class="bg-white">
      {@html qrcode}
    </div>
    {#if qrLogo}
      <div
        class="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 *:size-12"
      >
        <Avatar
          src={qrLogo}
          border="border-primary-500/50 border-2"
          background="bg-white"
          width="w-12"
        />
      </div>
    {/if}
  </div>
  <div
    class="text-surface-900-50-token mt-2 flex flex-row items-center justify-center gap-2 text-pretty break-all text-sm"
  >
    <span>{qrValue}</span>
    <TextClipboardButton textValue={qrValue} />
  </div>
</ModalCard>
