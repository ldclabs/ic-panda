<script lang="ts">
  import { onMount, type SvelteComponent } from 'svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import TextClipboard from '$lib/components/ui/TextClipboard.svelte'
  import TextTokenAmount from '$lib/components/ui/TextTokenAmount.svelte'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { authStore } from '$lib/stores/auth'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { signOut } from '$lib/services/auth'
  import { Principal } from '@dfinity/principal'
  import { AccountIdentifier } from '$lib/utils/account_identifier'
  import { getICPLedgerService, ICPLedgerAPI } from '$lib/canisters/icpledger'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let principal = $authStore.identity?.getPrincipal() || Principal.anonymous()
  let icpAddress = accountId(principal).toHex()

  let icp_balance = Promise.resolve(BigInt(0))
  let panda_balance = Promise.resolve(BigInt(0))

  const modalStore = getModalStore()

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

  onMount(async () => {
    const canister = await getICPLedgerService({
      identity: $authStore.identity
    })
    const icpAPI = new ICPLedgerAPI(canister)
    icp_balance = icpAPI.getBalanceOf(principal)
  })
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
    <ul
      class="list !mt-8 space-y-4 *:flex *:h-10 *:flex-row *:justify-between *:!rounded-md *:px-4 *:py-2"
    >
      <li
        class="*:flex *:flex-row *:content-center *:gap-3 hover:bg-primary-100/50"
      >
        <div class="max-w-[50%] leading-8">
          <span class="*:size-8"><IconIcLogo /></span>
          <span class="truncate">Internet Computer</span>
        </div>
        <TextTokenAmount
          selfClass="leading-8 *:min-w-14"
          token={ICPToken}
          amount={icp_balance}
        />
      </li>
      <li
        class="*:flex *:flex-row *:content-center *:gap-3 hover:bg-primary-100/50"
      >
        <div class="leading-8">
          <span class="*:size-8"><IconPanda /></span>
          <span>ICPanda</span>
        </div>
        <TextTokenAmount
          selfClass="leading-8 *:min-w-14"
          token={PANDAToken}
          amount={panda_balance}
        />
      </li>
    </ul>
    <footer class="!mt-8 {parent['regionFooter']}">
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
