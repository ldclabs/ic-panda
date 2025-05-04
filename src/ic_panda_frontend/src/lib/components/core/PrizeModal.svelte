<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/state'
  import {
    luckyPoolAPI,
    type ClaimPrizeOutput,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconCloseCircle from '$lib/components/icons/IconCloseCircle.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { challengeToken, signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { decodePrize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import { encodeCBOR } from '@ldclabs/cose-ts/utils'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import PrizeClaimed from './PrizeClaimed.svelte'
  import PrizeShow from './PrizeShow.svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let prizeCode: string = ''

  const airdropState = luckyPoolAPI.airdropStateStore
  const luckyPoolState = luckyPoolAPI.stateStore

  if (prizeCode.startsWith('http')) {
    const url = new URL(prizeCode)
    if (url.origin == APP_ORIGIN) {
      prizeCode = url.searchParams.get('prize') || ''
    } else {
      prizeCode = ''
    }
  }

  let cryptogram = prizeCode || page.url?.searchParams.get('prize') || ''
  let submitting = false
  let validating = decodePrize(cryptogram) != null
  let canClaim = true
  let meetRequirements = 3
  let prizeInfo: PrizeOutput | null = !validating
    ? null
    : {
        // placeholder
        id: new Uint8Array(),
        fee: 0n,
        issued_at: 0n,
        code: [''],
        kind: 1,
        memo: [],
        name: [],
        refund_amount: 0n,
        issuer: '',
        filled: 0,
        quantity: 100,
        expire: 7200n,
        ended_at: 0n,
        amount: 0n,
        sys_subsidy: 0n
      }
  let result: ClaimPrizeOutput

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  async function prizeCodeCopyPaste(e: Event) {
    e.preventDefault()

    if (cryptogram == '') {
      cryptogram = await navigator.clipboard.readText()
    } else {
      cryptogram = ''
    }
    checkValidity()
  }

  function checkValidity() {
    validating = decodePrize(cryptogram) != null
  }

  async function onFormSubmit(e: Event) {
    e.preventDefault()

    claimPrize()
  }

  async function claimPrize() {
    if (canClaim) {
      submitting = true
      try {
        const prize = decodePrize(cryptogram)
        const token = await challengeToken(
          {
            principal: principal.toUint8Array(),
            message: encodeCBOR(prize)
          },
          'claim_prize'
        )

        result = await luckyPoolAPI.claimPrize({
          code: cryptogram,
          challenge: token
        })

        luckyPoolAPI.refreshAllState()
      } catch (err: any) {
        submitting = false

        toastStore.trigger({
          autohide: false,
          hideDismiss: false,
          background: 'variant-filled-error',
          message: errMessage(err)
        })
      }
    }
  }

  function onFormChange(e: Event) {
    checkValidity()
  }

  function closePrizeShow() {
    modalStore.clear()

    if (page.url?.searchParams.get('prize')) {
      const query = page.url.searchParams
      query.delete('prize')
      goto(`?${query.toString()}`)
    }
  }

  onMount(async () => {
    checkValidity()

    if (validating) {
      prizeInfo = await luckyPoolAPI.prizeInfo(cryptogram)
      canClaim =
        prizeInfo.filled < prizeInfo.quantity && prizeInfo.ended_at == 0n
    }

    // Remove the prize query parameter from the URL
    if (!principal.isAnonymous() && page.url?.searchParams.get('prize')) {
      const query = page.url.searchParams
      query.delete('prize')
      goto(`?${query.toString()}`)
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    let _meetRequirements = 0
    if ($airdropState?.lucky_code[0]) {
      _meetRequirements = _meetRequirements | 1
    }
    if ($airdropState?.claimable && $airdropState?.claimable >= 50n) {
      _meetRequirements = _meetRequirements | 2
      meetRequirements = _meetRequirements
    }

    if (validating && (prizeInfo == null || prizeInfo.code[0] != cryptogram)) {
      luckyPoolAPI.prizeInfo(cryptogram).then((info) => {
        prizeInfo = info
        canClaim =
          prizeInfo.filled < prizeInfo.quantity && prizeInfo.ended_at == 0n
      })
    }
  }
</script>

<ModalCard {parent} width="w-[380px]">
  {#if prizeInfo && result}
    <PrizeClaimed {prizeInfo} {result} close={closePrizeShow} />
  {:else if prizeInfo && (principal.isAnonymous() || meetRequirements == 3)}
    <PrizeShow {prizeInfo} {claimPrize} close={closePrizeShow} />
  {:else}
    <h3 class="h3 !mt-0 text-center">🐼 🎁</h3>
    <div class="!mt-0 text-center text-xl font-bold">Get a Prize</div>
    <div class="m-auto mt-5 flex flex-col content-center">
      <p class="text-gray/50 text-sm">Meet requirements:</p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 1 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange-500 *:size-5"><IconCheckbox /></span>
        {/if}
        <span>Have a <b>lucky code</b> through airdrop</span>
      </p>
      <p class="mt-3 flex flex-row items-center gap-2">
        {#if meetRequirements != 2 && meetRequirements != 3}
          <span class="*:size-5"><IconCloseCircle /></span>
        {:else}
          <span class="text-orange-500 *:size-5"><IconCheckbox /></span>
        {/if}
        <span
          >Have at least <b
            >{Number($luckyPoolState?.airdrop_amount[0] || 10n) / 2} PANDA</b
          > in your wallet</span
        >
      </p>
    </div>
    <form
      class="m-auto !mt-6 flex flex-col content-center"
      on:input={onFormChange}
    >
      <label class="label">
        <span>Fill in prize code here:</span>
        <div class="relative">
          <input
            class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
            type="text"
            name="cryptogram"
            minlength="20"
            maxlength="50"
            bind:value={cryptogram}
            placeholder="Enter code"
            disabled={submitting}
            required
          />
          <button
            class="btn absolute top-0 right-0 outline-0"
            disabled={submitting}
            on:click={prizeCodeCopyPaste}
          >
            {#if cryptogram == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>
      </label>
    </form>
    <footer class="m-auto !my-6">
      {#if principal.isAnonymous()}
        <button
          class="btn flex w-full flex-row items-center gap-2 bg-gradient-to-r from-amber-300 to-red-500 text-white"
          on:click={() => signIn()}
        >
          <span>Login</span>
        </button>
      {:else}
        <button
          class="btn flex w-full flex-row items-center gap-2 text-white {meetRequirements <
          3
            ? 'bg-gray/20'
            : 'bg-gradient-to-r from-amber-300 to-red-500'}"
          disabled={submitting || !validating || meetRequirements != 3}
          on:click={onFormSubmit}
        >
          {#if submitting}
            <span class=""><IconCircleSpin /></span>
            <span>Processing...</span>
          {:else}
            <span>Claim Now</span>
          {/if}
        </button>
      {/if}
    </footer>
    <hr class="!border-gray/20 mx-[-24px] !mt-0 !border-t-1 !border-dashed" />
    <div class="m-auto !mt-5">
      <p class="text-gray/50 text-sm">To get the latest updates by following:</p
      >
      <div class="mt-3 flex flex-row justify-between">
        <a
          type="button"
          title="Follow on Twitter"
          class="btn btn-sm border-gray/10 rounded-xl border-[1px]"
          href="https://twitter.com/ICPandaDAO"
          target="_blank"
        >
          <span><IconX /></span>
          <span class="text-left">ICPanda DAO</span>
        </a>
        <a
          type="button"
          title="Join the Community"
          class="btn btn-sm border-gray/10 rounded-xl border-[1px]"
          href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
          target="_blank"
        >
          <span><IconOpenChat /></span>
          <span class="text-left">ICPanda Community</span>
        </a>
      </div>
    </div>
  {/if}
</ModalCard>
