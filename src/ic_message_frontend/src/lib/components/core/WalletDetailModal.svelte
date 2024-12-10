<script lang="ts">
  import { icpLedgerAPI } from '$lib/canisters/icpledger'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import {
    dmsgLedgerAPI,
    pandaLedgerAPI,
    TokenLedgerAPI
  } from '$lib/canisters/tokenledger'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconDmsg from '$lib/components/icons/IconDmsg.svelte'
  import IconExternalLinkLine from '$lib/components/icons/IconExternalLinkLine.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardPopup from '$lib/components/ui/TextClipboardPopup.svelte'
  import { authStore } from '$lib/stores/auth'
  import { getTokenPrice, type TokenPrice } from '$lib/stores/exchange'
  import { MyMessageState } from '$lib/stores/message'
  import type { LedgerAPI } from '$lib/types/token'
  import { shortId } from '$lib/utils/auth'
  import { getPriceNumber } from '$lib/utils/helper'
  import {
    DMSGToken,
    ICPToken,
    PANDAToken,
    TokenDisplay,
    type TokenInfo
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount, type Snippet, type SvelteComponent } from 'svelte'
  import ImportTokenModal from './ImportTokenModal.svelte'
  import TopupTokenModal from './TopupTokenModal.svelte'
  import TransferTokenModal from './TransferTokenModal.svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
  }

  type TokenInfoEx = TokenInfo & {
    api: LedgerAPI
    balance: bigint
    price: TokenPrice | null
  }

  let { parent }: Props = $props()

  const modalStore = getModalStore()
  let myState: MyMessageState | null = $state(null)
  let principal = $authStore.identity.getPrincipal()
  let myInfo: ProfileInfo | null = $state(null)

  let myTokens = $state<TokenInfoEx[]>([])
  let icpTokenInfo: TokenInfoEx = $state({
    ...ICPToken,
    api: icpLedgerAPI,
    balance: 0n,
    price: null
  })
  getTokenPrice(icpTokenInfo.canisterId).subscribe((price) => {
    icpTokenInfo.price = price
  })

  let pandaTokenInfo: TokenInfoEx = $state({
    ...PANDAToken,
    api: pandaLedgerAPI,
    balance: 0n,
    price: null
  })
  getTokenPrice(PANDAToken.canisterId).subscribe((price) => {
    pandaTokenInfo.price = price
  })

  let dmsgTokenInfo: TokenInfoEx = $state({
    ...DMSGToken,
    api: dmsgLedgerAPI,
    balance: 0n,
    price: null
  })

  getTokenPrice(DMSGToken.canisterId).subscribe((price) => {
    dmsgTokenInfo.price = price
  })

  function onClickToken(token: TokenInfoEx) {
    ;(modalStore as any).trigger2({
      type: 'component',
      component: {
        ref: TransferTokenModal,
        props: {
          token,
          principal
        }
      }
    })
  }

  function onClickImportToken() {
    ;(modalStore as any).trigger2({
      type: 'component',
      component: {
        ref: ImportTokenModal,
        props: {
          onsave: async (token: TokenInfoEx) => {
            if (
              !myState ||
              token.canisterId === icpTokenInfo.canisterId ||
              token.canisterId === pandaTokenInfo.canisterId ||
              token.canisterId === dmsgTokenInfo.canisterId
            ) {
              return
            }
            const tokenIds = myTokens.map((v) => v.canisterId)
            if (tokenIds.includes(token.canisterId)) {
              return
            }

            tokenIds.push(token.canisterId)
            const tokens = tokenIds.map((v) => Principal.fromText(v))
            const profile = await myState.agent.getProfile()
            await myState.agent.profileAPI.update_tokens(tokens)
            await myState.agent.setProfile({
              ...profile,
              tokens
            })
          }
        }
      }
    })
  }

  function onClickTopupPANDA() {
    ;(modalStore as any).trigger2({
      type: 'component',
      component: {
        ref: TopupTokenModal,
        props: {
          to: myState!.principal,
          token: PANDAToken,
          onfinish: async () => {
            pandaTokenInfo.balance = await pandaLedgerAPI.balance()
          }
        }
      }
    })
  }

  let deleteTokenSubmitting = $state('')
  async function onDeleteToken(id: string) {
    if (!myState) {
      return
    }
    let tokenIds = myTokens.map((v) => v.canisterId)
    if (!tokenIds.includes(id)) {
      return
    }

    deleteTokenSubmitting = id
    tokenIds = tokenIds.filter((v) => v !== id)
    const tokens = tokenIds.map((v) => Principal.fromText(v))
    const profile = await myState.agent.getProfile()
    await myState.agent.profileAPI.update_tokens(tokens)
    await myState.agent.setProfile({
      ...profile,
      tokens
    })
    myTokens = myTokens.filter((v) => v.canisterId !== id)
    deleteTokenSubmitting = ''
  }

  onMount(async () => {
    myState = await MyMessageState.load()
    if (!myState.principal.isAnonymous()) {
      icpLedgerAPI.balance().then((balance) => {
        icpTokenInfo.balance = balance
      })

      pandaLedgerAPI.balance().then((balance) => {
        pandaTokenInfo.balance = balance
      })

      dmsgLedgerAPI.balance().then((balance) => {
        dmsgTokenInfo.balance = balance
      })

      myInfo = await myState!.agent.getProfile().catch(() => null)
      if (myInfo) {
        const tokens = await myState.agent.loadTokens(myInfo.tokens)
        myTokens = tokens.map((token) => {
          const api = new TokenLedgerAPI(token)
          return {
            ...token,
            api,
            balance: 0n,
            price: null
          }
        })

        for (const token of myTokens) {
          token.api.balance().then((balance) => (token.balance = balance))
          getTokenPrice(token.canisterId).subscribe((price) => {
            token.price = price
          })
        }
      }
    }
  })
</script>

{#snippet icpLogo()}
  <span class="*:size-12"><IconIcLogo /></span>
{/snippet}

{#snippet pandaLogo()}
  <span class="*:size-12"><IconPanda /></span>
{/snippet}

{#snippet dmsgLogo()}
  <span class="*:size-12"><IconDmsg /></span>
{/snippet}

{#snippet tokenItem(
  token: TokenInfoEx,
  logo?: () => ReturnType<Snippet>,
  canDelete?: boolean
)}
  {@const tokenInfo = new TokenDisplay(token, token.balance)}
  {@const tokenValue = tokenInfo.num * (token.price?.priceUSD || 0)}
  <a
    type="button"
    href="/"
    class="bg-surface-hover-token btn relative grid grid-cols-[48px_1fr] rounded-xl px-4 {canDelete
      ? 'mr-4'
      : ''}"
    onclick={(ev) => {
      ev.preventDefault()
      ev.stopPropagation()
      onClickToken(token)
    }}
  >
    {#if logo}
      {@render logo()}
    {:else}
      <img class="size-12" alt={token.name} src={token.logo} />
    {/if}
    <div class="pl-2">
      <div class="flex flex-row justify-between">
        <span class="">{token.symbol}</span>
        <span class="">{tokenInfo.display()}</span>
      </div>
      <div class="flex flex-row justify-between text-sm text-surface-500">
        <span class="">{token.name}</span>
        {#if tokenValue > 0}
          <span class="">{'$' + getPriceNumber(tokenValue)}</span>
        {/if}
      </div>
    </div>
    {#if canDelete}
      <button
        type="button"
        class="absolute right-[-28px] top-4 p-1 text-neutral-500/50 hover:text-surface-900-50-token"
        disabled={deleteTokenSubmitting == token.canisterId}
        onclick={(ev) => {
          ev.preventDefault()
          ev.stopPropagation()
          onDeleteToken(token.canisterId)
        }}
      >
        <span class="*:size-5">
          {#if deleteTokenSubmitting == token.canisterId}
            <IconCircleSpin />
          {:else}
            <IconDeleteBin />
          {/if}
        </span>
      </button>
    {/if}
  </a>
{/snippet}

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Wallet</div>
  <div class="!mt-6 flex flex-col gap-3 rounded-xl bg-surface-500/20 px-4 py-3">
    <TextClipboardPopup
      textLable="Address:"
      textName={shortId(principal.toString())}
      textValue={principal.toString()}
    />
  </div>
  <button
    type="button"
    class="variant-filled-primary btn flex w-full flex-row items-center justify-center rounded-xl px-4 py-3"
    onclick={onClickTopupPANDA}
  >
    <span>Topup PANDA via OISY Wallet</span>
  </button>
  <hr class="!border-t-1 !border-gray/20 mx-[-24px] !mt-6 !border-dashed" />
  <div class="!mt-2 flex flex-col gap-0">
    {@render tokenItem(icpTokenInfo, icpLogo)}
    {@render tokenItem(pandaTokenInfo, pandaLogo)}
    {@render tokenItem(dmsgTokenInfo, dmsgLogo)}
    {#each myTokens as token (token.canisterId)}
      {@render tokenItem(token, undefined, true)}
    {/each}
    <button
      type="button"
      class="bg-surface-hover-token btn flex w-full flex-row items-center justify-center rounded-xl px-4"
      onclick={onClickImportToken}
    >
      <span class="*:size-5"><IconAdd /></span>
      <span>Import Token</span>
    </button>
  </div>
  <div class="mt-4 flex flex-col gap-3 px-4 py-3 text-sm">
    <a
      type="button"
      class="flex w-fit flex-row items-center gap-2 text-primary-500"
      target="_blank"
      href="https://oisy.com/transactions/?token=ICPanda&network=ICP"
    >
      <span class="*:size-5"><IconExternalLinkLine /></span>
      <span class="hover:underline">Get PANDA from OISY (Fiat Money)</span>
    </a>
    <a
      type="button"
      class="flex w-fit flex-row items-center gap-2 text-primary-500"
      target="_blank"
      href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
    >
      <span class="*:size-5"><IconExternalLinkLine /></span>
      <span class="hover:underline">Get PANDA from ICPswap</span>
    </a>
  </div>
</ModalCard>
