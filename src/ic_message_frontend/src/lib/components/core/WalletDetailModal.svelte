<script lang="ts">
  import { icpLedgerAPI } from '$lib/canisters/icpledger'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import { pandaLedgerAPI, TokenLedgerAPI } from '$lib/canisters/tokenledger'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconExternalLinkLine from '$lib/components/icons/IconExternalLinkLine.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardPopup from '$lib/components/ui/TextClipboardPopup.svelte'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$lib/stores/message'
  import type { LedgerAPI } from '$lib/types/token'
  import { shortId } from '$lib/utils/auth'
  import {
    ICPToken,
    PANDAToken,
    TokenDisplay,
    type TokenInfo
  } from '$lib/utils/token'
  import { Principal } from '@dfinity/principal'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { onMount, type Snippet, type SvelteComponent } from 'svelte'
  import ImportTokenModal from './ImportTokenModal.svelte'
  import TransferTokenModal from './TransferTokenModal.svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
  }

  type TokenInfoEx = TokenInfo & { api: LedgerAPI; balance: bigint }

  let { parent }: Props = $props()

  const modalStore = getModalStore()
  let myState: MyMessageState | null = $state(null)
  let principal = $authStore.identity.getPrincipal()
  let myInfo: ProfileInfo | null = $state(null)

  let myTokens = $state<TokenInfoEx[]>([])
  let icpTokenInfo: TokenInfoEx = $state({
    ...ICPToken,
    api: icpLedgerAPI,
    balance: 0n
  })
  let pandaTokenInfo: TokenInfoEx = $state({
    ...PANDAToken,
    api: pandaLedgerAPI,
    balance: 0n
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
              token.canisterId === pandaTokenInfo.canisterId
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
      icpTokenInfo.balance = await icpLedgerAPI.balance()
      pandaTokenInfo.balance = await pandaLedgerAPI.balance()

      myInfo = await myState.agent.getProfile().catch(() => null)
      if (myInfo) {
        const tokens = await myState.agent.loadTokens(myInfo.tokens)
        myTokens = await Promise.all(
          tokens.map((token) => {
            const api = new TokenLedgerAPI(token)
            return api.balance().then((balance) => ({ ...token, api, balance }))
          })
        )
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

{#snippet tokenItem(token: TokenInfoEx, logo?: Snippet, canDelete?: boolean)}
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
        <span class="">{new TokenDisplay(token, token.balance).display()}</span>
      </div>
      <div class="flex flex-row justify-between text-sm text-surface-500">
        <span class="">{token.name}</span>
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
  <div class="!mt-2 flex flex-col gap-0">
    {@render tokenItem(icpTokenInfo, icpLogo)}
    {@render tokenItem(pandaTokenInfo, pandaLogo)}
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
  <div class="mt-4 flex flex-col gap-3 px-4 py-3">
    <a
      type="button"
      class="flex w-fit flex-row items-center gap-2 text-primary-500"
      target="_blank"
      href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
    >
      <span class="*:size-5"><IconExternalLinkLine /></span>
      <span class="hover:underline">Buy $PANDA on ICPswap</span>
    </a>
    <a
      type="button"
      class="flex w-fit flex-row items-center gap-2 text-primary-500"
      target="_blank"
      href="https://oisy.com/transactions/?token=ICPanda&network=ICP"
    >
      <span class="*:size-5"><IconExternalLinkLine /></span>
      <span class="hover:underline">Buy $PANDA on OISY Wallet</span>
    </a>
  </div>
</ModalCard>
