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
  <div
    class="card relative mt-12 w-full max-w-[420px] space-y-4 rounded-2xl bg-white p-6 shadow-xl max-md:mt-8"
  >
    <button
      class="z-1 btn btn-icon absolute right-2 top-2 text-surface-600/60 *:scale-125 hover:scale-110 max-md:right-2 max-md:top-2"
      on:click={parent['onClose']}
    >
      <IconClose />
    </button>
    <header class="!mt-0 text-center text-xl font-bold">Account</header>
    <div class="flex flex-col gap-3 rounded-xl bg-surface-50/50 px-4 py-3">
      <TextClipboard
        textLable="Principal:"
        textName={shortId(principal.toString())}
        textValue={principal.toString()}
      />
      <TextClipboard
        textLable="ICP Address:"
        textName={shortId(icpAddress)}
        textValue={icpAddress}
      />
    </div>

    <Accordion
      hover="hover:border-surface-200/80"
      padding="px-0 py-4"
      spacing="space-y-0"
      regionControl="border-b border-surface-50/50 !rounded-none"
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
  </div>
{/if}
