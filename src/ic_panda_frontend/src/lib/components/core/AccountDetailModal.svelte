<script lang="ts">
  import { icpLedgerAPI } from '$lib/canisters/icpledger'
  import { luckyPoolAPI, type NameOutput } from '$lib/canisters/luckypool'
  import { tokenLedgerAPI } from '$lib/canisters/tokenledger'
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
  import { shortId } from '$lib/utils/helper'
  import { ICPToken, PANDAToken } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import {
    Accordion,
    AccordionItem,
    getModalStore
  } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import { type Readable } from 'svelte/store'
  import NameModal from './NameModal.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let principal = $authStore.identity.getPrincipal()
  let icpAddress = accountId(principal).toHex()

  let icpBalance = Promise.resolve(0n)
  let pandaBalance = Promise.resolve(0n)
  let availableICPBalance = 0n
  let availablePandaBalance = 0n
  let nameState: Readable<NameOutput | null>

  const modalStore = getModalStore()

  function onLogoutHandler(): void {
    signOut().then(() => {
      parent && parent['onClose']()
    })
  }

  function editName(mode: 0 | 1 | 2) {
    modalStore.close()
    modalStore.trigger({
      type: 'component',
      component: {
        ref: NameModal,
        props: {
          nameEditMode: mode,
          availablePandaBalance: availablePandaBalance,
          nameState: nameState
        }
      }
    })
  }

  function accountId(principal: Principal | undefined): AccountIdentifier {
    return AccountIdentifier.fromPrincipal({
      principal: principal || Principal.anonymous()
    })
  }

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

    await luckyPoolAPI.refreshNameState()
    nameState = luckyPoolAPI.nameStateStore

    availableICPBalance = await icpBalance
    availablePandaBalance = await pandaBalance
  })

  $: name = $nameState?.name || ''
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Account</div>
  {#if name != ''}
    <div class="!mt-5 flex flex-col gap-2">
      <div class="">Name</div>
      <div class="relative">
        <input
          class="input truncate rounded-xl border-gray/10 bg-white/20 pr-36 invalid:input-warning hover:bg-white/90"
          type="text"
          name="pandaName"
          bind:value={name}
          disabled={true}
          placeholder="Register your brand name"
        />
        {#if name == ''}
          <button
            class="btn absolute right-0 top-0 text-panda outline-0"
            on:click={() => editName(0)}
          >
            <span>Register</span>
          </button>
        {:else}
          <div class="absolute right-0 top-0 flex flex-row items-center">
            <button
              class="btn px-1 text-gray/50 outline-0"
              on:click={() => editName(1)}
            >
              <span>Update</span>
            </button>
            <button
              class="btn px-2 text-warning-500 outline-0"
              on:click={() => editName(2)}
            >
              <span>Unregister</span>
            </button>
          </div>
        {/if}
      </div>
    </div>
    <hr class="!border-t-1 mx-[-24px] !mt-6 !border-dashed !border-gray/20" />
  {/if}
  <div class="!mt-6 flex flex-col gap-3 rounded-xl bg-gray/5 px-4 py-3">
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
    <AccordionItem regionControl="outline-0">
      <svelte:fragment slot="lead">
        <span class="*:size-8"><IconIcLogo /></span>
      </svelte:fragment>
      <svelte:fragment slot="summary">
        <div class="flex flex-row items-center justify-between leading-8">
          <span class="">Internet Computer</span>
          <TextTokenAmount
            class="flex flex-row items-center gap-2 pl-4"
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
    <AccordionItem regionControl="outline-0">
      <svelte:fragment slot="lead">
        <span class="*:size-8"><IconPanda /></span>
      </svelte:fragment>
      <svelte:fragment slot="summary">
        <div class="flex flex-row items-center justify-between leading-8">
          <span class="">ICPanda</span>
          <TextTokenAmount
            class="flex flex-row items-center gap-2 pl-4"
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
