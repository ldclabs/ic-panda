<script lang="ts">
  import { onMount, type SvelteComponent } from 'svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import TextClipboard from '$lib/components/ui/TextClipboard.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import SendTokenForm from '$lib/components/ui/SendTokenForm.svelte'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { authStore } from '$lib/stores/auth'
  import type { SendTokenArgs } from '$lib/types/token'
  import {
    getModalStore,
    Accordion,
    AccordionItem
  } from '@skeletonlabs/skeleton'
  import { signOut } from '$lib/services/auth'
  import { Principal } from '@dfinity/principal'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import { getICPLedgerService, ICPLedgerAPI } from '$lib/canisters/icpledger'
  import {
    getTokenLedgerService,
    TokenLedgerAPI
  } from '$lib/canisters/tokenledger'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  const modalStore = getModalStore()

  let principal = $authStore.identity?.getPrincipal() || Principal.anonymous()
  let icpAddress = accountId(principal).toHex()

  let icpBalance = Promise.resolve(0n)
  let pandaBalance = Promise.resolve(0n)
  let availableICPBalance = 0n
  let availablePandaBalance = 0n

  let icpAPI: ICPLedgerAPI
  let tokenAPI: TokenLedgerAPI
  onMount(async () => {
    const icpCanister = await getICPLedgerService({
      identity: $authStore.identity
    })

    icpAPI = new ICPLedgerAPI(icpCanister)
    icpBalance = icpAPI.getBalanceOf(principal)

    const tokenCanister = await getTokenLedgerService({
      identity: $authStore.identity
    })

    tokenAPI = new TokenLedgerAPI(tokenCanister)
    pandaBalance = tokenAPI.getBalanceOf(principal)

    availableICPBalance = await icpBalance
    availablePandaBalance = await pandaBalance
  })

  function onLogoutHandler(): void {
    signOut().then(() => {
      modalStore.close()
    })
  }

  function shortId(id: string): string {
    return id.length > 14 ? id.slice(0, 7) + '...' + id.slice(-7) : id
  }

  function accountId(principal: Principal | undefined): AccountIdentifier {
    return AccountIdentifier.fromPrincipal({
      principal: principal || Principal.anonymous()
    })
  }

  function handleICPTransfer(args: SendTokenArgs) {
    return icpAPI.transfer(args.to, args.tokenAmount)
  }

  function handlePANDATransfer(args: SendTokenArgs) {
    return tokenAPI.transfer(args.to, args.tokenAmount)
  }
</script>

{#if $modalStore[0]}
  <!-- This is a hack to fix the focus issue -->
  <button class="hidden"></button>
  <div class="card w-modal relative mt-8 space-y-4 bg-white/95 p-4 shadow-xl">
    <button
      class="z-1 variant-filled-surface btn btn-icon btn-icon-sm absolute -right-4 -top-4 hover:scale-110 max-md:right-2 max-md:top-2"
      on:click={parent['onClose']}
    >
      <IconClose />
    </button>
    <header class="!mt-0 text-center text-xl font-bold">Account</header>
    <div class="flex flex-col rounded-md bg-primary-100/50 px-4 py-2">
      <div class="flex flex-row gap-2">
        <span class="w-24 text-right">Principal:</span>
        <TextClipboard
          textName={shortId(principal.toString())}
          textValue={principal.toString()}
        />
      </div>
      <div class="mt-2 flex flex-row gap-2">
        <span class="w-24 text-right">ICP Address:</span>
        <TextClipboard textName={shortId(icpAddress)} textValue={icpAddress} />
      </div>
    </div>

    <Accordion>
      <AccordionItem>
        <svelte:fragment slot="lead">
          <span class="*:size-8"><IconIcLogo /></span>
        </svelte:fragment>
        <svelte:fragment slot="summary">
          <div class="flex flex-row justify-between leading-8">
            <span class="max-w-[40%] truncate">Internet Computer</span>
            <TextTokenAmount token={ICPToken} amount={icpBalance} />
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
          <div class="flex flex-row justify-between leading-8">
            <span class="max-w-[40%] truncate">ICPanda</span>
            <TextTokenAmount token={PANDAToken} amount={pandaBalance} />
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
    <footer class="!mt-8 {parent['regionFooter']} !justify-start">
      <button
        class="variant-filled-surface btn btn-sm"
        on:click={onLogoutHandler}
      >
        <IconLogout />
        <span>Logout</span>
      </button>
    </footer>
  </div>
{/if}
