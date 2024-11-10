<script lang="ts">
  import { icpLedgerAPI } from '$lib/canisters/icpledger'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import SendTokenForm from '$lib/components/ui/SendTokenForm.svelte'
  import TextClipboardPopup from '$lib/components/ui/TextClipboardPopup.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import { authStore } from '$lib/stores/auth'
  import type { SendTokenArgs } from '$lib/types/token'
  import { shortId } from '$lib/utils/auth'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { Accordion, AccordionItem } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
  }

  let { parent }: Props = $props()

  let principal = $authStore.identity.getPrincipal()

  let icpBalance = $state(Promise.resolve(0n))
  let pandaBalance = $state(Promise.resolve(0n))
  let availableICPBalance = $state(0n)
  let availablePandaBalance = $state(0n)

  async function handleICPTransfer(args: SendTokenArgs) {
    const idx = await icpLedgerAPI.transfer(args.to, args.amount)
    icpBalance = icpLedgerAPI.balance()
    availableICPBalance = await icpBalance
    return idx
  }

  async function handlePANDATransfer(args: SendTokenArgs) {
    const idx = await tokenLedgerAPI.transfer(args.to, args.amount)
    pandaBalance = tokenLedgerAPI.balance()
    availablePandaBalance = await pandaBalance
    return idx
  }

  onMount(async () => {
    icpBalance = icpLedgerAPI.balance()
    pandaBalance = tokenLedgerAPI.balance()

    availableICPBalance = await icpBalance
    availablePandaBalance = await pandaBalance
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Wallet</div>
  <div class="!mt-6 flex flex-col gap-3 rounded-xl bg-surface-500/20 px-4 py-3">
    <TextClipboardPopup
      textLable="Address:"
      textName={shortId(principal.toString())}
      textValue={principal.toString()}
    />
  </div>

  <Accordion
    hover="hover:border-surface-200/80"
    padding="px-0 py-4"
    spacing="space-y-0"
    regionControl="border-b border-gray/10 !rounded-none"
  >
    <AccordionItem regionControl="outline-0">
      {#snippet lead()}
        <span class="*:size-8"><IconIcLogo /></span>
      {/snippet}
      {#snippet summary()}
        <div class="flex flex-row items-center justify-between leading-8">
          <span class="">Internet Computer</span>
          <TextTokenAmount
            class="flex flex-row items-center gap-2 pl-4"
            token={ICPToken}
            amount={icpBalance}
          />
        </div>
      {/snippet}
      {#snippet content()}
        <SendTokenForm
          sendFrom={principal}
          availableBalance={availableICPBalance}
          token={ICPToken}
          onSubmit={handleICPTransfer}
        />
      {/snippet}
    </AccordionItem>
    <AccordionItem regionControl="outline-0">
      {#snippet lead()}
        <span class="*:size-8"><IconPanda /></span>
      {/snippet}
      {#snippet summary()}
        <div class="flex flex-row items-center justify-between leading-8">
          <span class="">ICPanda</span>
          <TextTokenAmount
            class="flex flex-row items-center gap-2 pl-4"
            token={PANDAToken}
            amount={pandaBalance}
          />
        </div>
      {/snippet}
      {#snippet content()}
        <SendTokenForm
          sendFrom={principal}
          availableBalance={availablePandaBalance}
          token={PANDAToken}
          onSubmit={handlePANDATransfer}
        />
      {/snippet}
    </AccordionItem>
  </Accordion>
</ModalCard>
