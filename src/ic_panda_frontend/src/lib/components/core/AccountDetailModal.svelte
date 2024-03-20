<script lang="ts">
  import { ICPLedgerAPI, icpLedgerAPIAsync } from '$lib/canisters/icpledger'
  import {
    TokenLedgerAPI,
    tokenLedgerAPIAsync
  } from '$lib/canisters/tokenledger'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import SendTokenForm from '$lib/components/ui/SendTokenForm.svelte'
  import TextClipboardPopup from '$lib/components/ui/TextClipboardPopup.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import { signOut } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import type { SendTokenArgs } from '$lib/types/token'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import { shortId } from '$lib/utils/auth'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { Accordion, AccordionItem } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let principal = $authStore.identity.getPrincipal()
  let icpAddress = accountId(principal).toHex()

  let icpBalance = Promise.resolve(0n)
  let pandaBalance = Promise.resolve(0n)
  let availableICPBalance = 0n
  let availablePandaBalance = 0n

  let icpLedgerAPI: ICPLedgerAPI
  let tokenLedgerAPI: TokenLedgerAPI

  function onLogoutHandler(): void {
    signOut().then(() => {
      parent && parent['onClose']()
    })
  }

  function accountId(principal: Principal | undefined): AccountIdentifier {
    return AccountIdentifier.fromPrincipal({
      principal: principal || Principal.anonymous()
    })
  }

  async function handleICPTransfer(args: SendTokenArgs) {
    const idx = await icpLedgerAPI.transfer(args.to, args.tokenAmount)
    icpBalance = icpLedgerAPI.balance()
    availableICPBalance = await icpBalance
    return idx
  }

  async function handlePANDATransfer(args: SendTokenArgs) {
    const idx = await tokenLedgerAPI.transfer(args.to, args.tokenAmount)
    pandaBalance = tokenLedgerAPI.balance()
    availablePandaBalance = await pandaBalance
    return idx
  }

  onMount(async () => {
    icpLedgerAPI = await icpLedgerAPIAsync()
    icpBalance = icpLedgerAPI.balance()

    tokenLedgerAPI = await tokenLedgerAPIAsync()
    pandaBalance = tokenLedgerAPI.balance()

    availableICPBalance = await icpBalance
    availablePandaBalance = await pandaBalance
  })
</script>

<ModalCard {parent}>
  <header class="!mt-0 text-center text-xl font-bold">Account</header>
  <div class="flex flex-col gap-3 rounded-xl bg-gray/5 px-4 py-3">
    <TextClipboardPopup
      textLable="Principal:"
      textName={shortId(principal.toString())}
      textValue={principal.toString()}
    />
    <TextClipboardPopup
      textLable="ICP Address:"
      textName={shortId(icpAddress)}
      textValue={icpAddress}
    />
  </div>

  <Accordion
    hover="hover:border-surface-200/80"
    padding="px-0 py-4"
    spacing="space-y-0"
    regionControl="border-b border-gray/10 !rounded-none"
  >
    <AccordionItem>
      <svelte:fragment slot="lead">
        <span class="*:size-8"><IconIcLogo /></span>
      </svelte:fragment>
      <svelte:fragment slot="summary">
        <div class="relative leading-8">
          <span class="max-w-[80%] truncate">Internet Computer</span>
          <TextTokenAmount
            class="absolute right-0 top-0 float-right flex flex-row items-center gap-2 bg-white pl-4"
            token={ICPToken}
            amount={icpBalance}
          />
        </div>
      </svelte:fragment>
      <svelte:fragment slot="content">
        <SendTokenForm
          sendFrom={principal}
          availableBalance={availableICPBalance}
          token={ICPToken}
          onSubmit={handleICPTransfer}
        />
      </svelte:fragment>
    </AccordionItem>
    <AccordionItem>
      <svelte:fragment slot="lead">
        <span class="*:size-8"><IconPanda /></span>
      </svelte:fragment>
      <svelte:fragment slot="summary">
        <div class="relative leading-8">
          <span class="max-w-[40%] truncate">ICPanda</span>
          <TextTokenAmount
            class="absolute right-0 top-0 float-right flex flex-row items-center gap-2 bg-white pl-4"
            token={PANDAToken}
            amount={pandaBalance}
          />
        </div>
      </svelte:fragment>
      <svelte:fragment slot="content">
        <SendTokenForm
          sendFrom={principal}
          availableBalance={availablePandaBalance}
          token={PANDAToken}
          onSubmit={handlePANDATransfer}
        />
      </svelte:fragment>
    </AccordionItem>
  </Accordion>
  <footer class="!mt-8 {parent['regionFooter']} !justify-center">
    <button class="variant-filled btn" on:click={onLogoutHandler}>
      <IconLogout />
      <span>Logout</span>
    </button>
  </footer>
</ModalCard>
