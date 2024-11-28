<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import SendTokenForm from '$lib/components/ui/SendTokenForm.svelte'
  import type { LedgerAPI, SendTokenArgs } from '$lib/types/token'
  import { type TokenInfo } from '$lib/utils/token'
  import type { Principal } from '@dfinity/principal'
  import { type SvelteComponent } from 'svelte'

  // Props

  type TokenInfoEx = TokenInfo & { api: LedgerAPI; balance: bigint }

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    token: TokenInfoEx
    principal: Principal
  }

  let { parent, principal, token }: Props = $props()

  async function handleTokenTransfer(args: SendTokenArgs) {
    const idx = await token.api.transfer(args.to, args.amount)
    token.balance = await token.api.balance()
    return idx
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Transfer {token.symbol}</div>
  <div class="mx-auto !mt-6 space-y-4">
    <SendTokenForm
      sendFrom={principal}
      availableBalance={token.balance}
      {token}
      onSubmit={handleTokenTransfer}
    />
  </div>
</ModalCard>
