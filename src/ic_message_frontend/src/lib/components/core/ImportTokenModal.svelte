<script lang="ts">
  import { TokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconExternalLinkLine from '$lib/components/icons/IconExternalLinkLine.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import type { LedgerAPI } from '$lib/types/token'
  import { TokenDisplay, type TokenInfo } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { tick, type SvelteComponent } from 'svelte'

  // Props

  type TokenInfoEx = TokenInfo & { api: LedgerAPI; balance: bigint }

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    onsave: (token: TokenInfoEx) => Promise<void>
  }

  let { parent, onsave }: Props = $props()

  let ledgerId = $state('')
  let ledgerErr = $state('')
  let token: TokenInfoEx | null = $state(null)
  let submitting = $state(false)

  async function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    ledgerErr = ''
    let id: Principal | null = null
    token = null
    try {
      id = Principal.fromText(ledgerId)
    } catch (e) {}

    if (!id) {
      return
    }

    try {
      submitting = true
      await tick()
      const api = await TokenLedgerAPI.fromID(id)
      const balance = await api.balance()
      token = { ...api.token, api, balance }
    } catch (e) {
      token = null
      ledgerErr = 'Token not found'
    }

    submitting = false
  }

  async function onclick(e: Event) {
    if (token) {
      submitting = true
      await onsave(token)
      parent['onClose'] && parent['onClose']()
    }
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Import token</div>

  <form
    class="m-auto !mt-4 flex flex-col content-center"
    oninput={onFormChange}
  >
    <div class="">
      <p class="">
        You can import a new token to wallet by providing its ledger canister
        ID.
      </p>
      <a
        type="button"
        class="mt-2 flex w-fit flex-row items-center gap-2 text-primary-500"
        target="_blank"
        href="https://internetcomputer.org/docs/current/developer-docs/daos/nns/using-the-nns-dapp/nns-dapp-importing-tokens"
      >
        <span class="*:size-5"><IconExternalLinkLine /></span>
        <span class="hover:underline">How to find ICRC token ledger</span>
      </a>
    </div>
    <div class="relative mt-4">
      <input
        class="border-gray/10 input truncate rounded-xl bg-white/20 invalid:input-warning"
        type="text"
        name="ledgerId"
        maxlength="27"
        data-1p-ignore
        bind:value={ledgerId}
        disabled={submitting}
        placeholder="_____-_____-_____-_____-cai"
        required
      />
    </div>
    {#if token}
      <div class="mt-4 grid grid-cols-[48px_1fr]">
        <img class="size-12" alt={token.name} src={token.logo} />
        <div class="pl-2">
          <div class="flex flex-row justify-between">
            <span class="">{token.symbol}</span>
            <span class=""
              >{new TokenDisplay(token, token.balance).display()}</span
            >
          </div>
          <div class="flex flex-row justify-between text-sm text-surface-500">
            <span class="">{token.name}</span>
          </div>
        </div>
      </div>
    {:else if ledgerErr}
      <div class="mt-4">
        <p class="text-error-500">{ledgerErr}</p>
      </div>
    {/if}
  </form>
  <footer class="m-auto !mt-6">
    <button
      class="variant-filled-primary btn w-full"
      disabled={submitting || !token}
      {onclick}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>Processing...</span>
      {:else}
        <span>Import</span>
      {/if}
    </button>
  </footer>
</ModalCard>
