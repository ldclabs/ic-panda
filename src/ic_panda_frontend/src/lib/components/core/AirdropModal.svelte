<script lang="ts">
  import { page } from '$app/stores'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
  import IconChatSmile from '$lib/components/icons/IconChatSmile.svelte'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { XAuth } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { errMessage } from '$lib/types/result'
  import { shortId } from '$lib/utils/auth'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let dmChallenge = false
  let oAuthSubmitting = false
  let submitting = false
  let validating = false
  let challenge = ''
  let cryptogram = ''
  let luckyPoolAPI: LuckyPoolAPI
  let luckyCode = $page.url.searchParams.get('ref') || ''
  let result: AirdropState
  let defaultClaimable = 10
  let principal = $authStore.identity.getPrincipal()

  const toastStore = getToastStore()
  const luckyLink = `${APP_ORIGIN}/?ref=`

  async function onFormSubmit(e: Event) {
    e.preventDefault()

    submitting = true
    try {
      result = await luckyPoolAPI.airdrop({
        challenge: challenge,
        code: cryptogram,
        lucky_code: luckyCode != '' ? [luckyCode] : [],
        recaptcha: []
      })
    } catch (err: any) {
      oAuthSubmitting = false
      challenge = ''
      cryptogram = ''
      submitting = false
      validating = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
    await luckyPoolAPI.refreshAllState()
  }

  function onFormChange(e: Event) {
    checkValidity()
  }

  function checkValidity() {
    validating =
      (challenge != '' || cryptogram != '') &&
      (luckyCode == '' || luckyCode.length == 6)
  }

  async function airdropCodeCopyPaste(e: Event) {
    e.preventDefault()

    if (cryptogram == '') {
      cryptogram = await navigator.clipboard.readText()
      checkValidity()
    } else {
      cryptogram = ''
    }
  }

  async function luckyCodeCopyPaste(e: Event) {
    e.preventDefault()

    if (luckyCode == '') {
      luckyCode = await navigator.clipboard.readText()
      checkValidity()
    } else {
      luckyCode = ''
    }
  }

  async function xAuth() {
    dmChallenge = false
    if (challenge != '') {
      return
    }

    oAuthSubmitting = true
    try {
      challenge = await XAuth.authorize(principal)
      checkValidity()
    } catch (err: any) {
      oAuthSubmitting = false
      challenge = ''
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
    const defaultAirdrop = await luckyPoolAPI.defaultAirdropState()
    defaultClaimable = Number(defaultAirdrop.claimable / PANDAToken.one)
  })
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
      <IconCheckbox />
    </div>
    <div class="text-center">
      <p class="mt-4">
        <span>
          You have successfully claimed <b
            >{formatNumber(
              Number(result.claimable) / Number(PANDAToken.one)
            )}</b
          > PANDA tokens.
        </span>
      </p>
      <p class="mt-4">
        <span>Your lucky code:</span>
        <span class="text-panda">{result.lucky_code[0]}</span>
        <TextClipboardButton textValue={result.lucky_code[0] || ''} />
      </p>
      <p class="mt-4">
        <span>Link:</span>
        <span>
          {luckyLink + result.lucky_code[0]}
        </span>
        <TextClipboardButton textValue={luckyLink + result.lucky_code[0]} />
      </p>
      <p class="mt-4 text-left">
        <span>Share your lucky code to with others.</span>
        <br />
        <span>
          When a new user claims the airdrop using your lucky code, you'll also
          receive an additional <b>{defaultClaimable / 2}</b> tokens per user.
        </span>
      </p>
    </div>
  {:else}
    <h3 class="h3 !mt-1 text-center">🪂 🎁</h3>
    <div class="text-center text-xl font-bold">Get the Airdrop</div>
    <div class="m-auto !mt-0 flex w-80 flex-col content-center">
      <h6 class="h6 mb-4 mt-5 text-center font-bold">
        <span>STEP 1: Get verified</span>
      </h6>
      <button
        class="variant-filled btn w-full rounded-xl {challenge != ''
          ? 'bg-panda'
          : ''}"
        disabled={oAuthSubmitting || submitting}
        on:click={xAuth}
      >
        {#if challenge != ''}
          <span class=""><IconCheckbox /></span>
        {:else}
          <span><IconX /></span>
        {/if}
        <span class="">To be Verified by X account</span>
      </button>
      <button
        class="btn text-gray/50 outline-0 {dmChallenge ? 'hidden' : ''}"
        disabled={oAuthSubmitting || submitting || challenge != ''}
        on:click={() => {
          dmChallenge = !dmChallenge
        }}
      >
        <span class="">Another option</span>
        <span><IconArrowDownLine /></span>
      </button>
      <div
        class="mt-4 rounded-xl bg-gray/5 {!dmChallenge
          ? 'collapse h-0'
          : 'visible mb-6'}"
      >
        <div class="border-b-[1px] border-gray/10 p-4">
          <p class="flex flex-row items-center gap-1 font-medium">
            <span>Or DM your principal to</span>
            <a
              title="DM us on Twitter"
              class="text-pink-500"
              href="https://twitter.com/ICPandaDAO"
              target="_blank">ICPanda X</a
            >
            <span class="text-pink-500 *:size-5"><IconChatSmile /></span>
          </p>
          <p class="text-sm text-gray/50"
            >We will reply to you with the airdrop code in <b
              class="text-pink-500">1-2 days</b
            >.</p
          >
        </div>
        <div class="flex flex-row items-center justify-between gap-1 p-4">
          <span class="font-medium">Principal:</span>
          <span class="truncate text-gray/50"
            >{shortId(principal.toString())}</span
          >
          <TextClipboardButton textValue={principal.toString()} />
        </div>
      </div>
    </div>

    <hr class="!border-t-1 mx-[-24px] !mt-0 !border-dashed !border-gray/20" />
    <form
      class="m-auto !mt-0 flex w-80 flex-col content-center"
      on:input={onFormChange}
    >
      <h6 class="h6 mb-4 mt-6 text-center font-bold">
        <span>STEP 2 : Fill-in airdrop code</span>
      </h6>
      <label class="label {!dmChallenge ? 'collapse h-0' : 'visible mb-2'}">
        <span>Airdrop code:</span>
        <div class="relative">
          <input
            class="input truncate rounded-xl bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
            type="text"
            name="cryptogram"
            minlength="20"
            maxlength="50"
            bind:value={cryptogram}
            placeholder="Enter code"
            disabled={submitting}
          />
          <button
            class="btn absolute right-0 top-0 outline-0"
            disabled={submitting}
            on:click={airdropCodeCopyPaste}
          >
            {#if cryptogram == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>

        <span class="text-sm text-pink-500"
          >You can also get this code by participating in lucky draw.</span
        >
      </label>
      <label class="label">
        <span>Lucky Code (Optinal):</span>
        <div class="relative">
          <input
            class="input truncate rounded-xl bg-white/20 pr-16 invalid:input-warning hover:bg-white/90"
            type="text"
            name="luckyCode"
            minlength="6"
            maxlength="6"
            bind:value={luckyCode}
            placeholder="Enter code"
            disabled={submitting}
          />
          <button
            class="btn absolute right-0 top-0 outline-0"
            disabled={submitting}
            on:click={luckyCodeCopyPaste}
          >
            {#if luckyCode == ''}
              <span>Paste</span>
            {:else}
              <span class="*:scale-90"><IconDeleteBin /></span>
            {/if}
          </button>
        </div>
      </label>
    </form>
    <footer class="m-auto !mt-6 w-80">
      <button
        class="btn flex w-full flex-row items-center gap-2 bg-pink-500 text-white"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Claim</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
