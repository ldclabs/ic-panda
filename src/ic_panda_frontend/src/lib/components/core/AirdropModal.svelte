<script lang="ts">
  import { page } from '$app/state'
  import { luckyPoolAPI, type AirdropState } from '$lib/canisters/luckypool'
  import IconArrowDownLine from '$lib/components/icons/IconArrowDownLine.svelte'
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
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let dmChallenge = false
  let oAuthSubmitting = false
  let submitting = false
  let validating = false
  let challenge = ''
  let cryptogram = ''
  let luckyCode = page?.url?.searchParams.get('ref') || ''
  let result: AirdropState
  let luckyPoolState = luckyPoolAPI.stateStore
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
        message: errMessage(err).replaceAll('\\n', '<br />')
      })
    }
  }
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-panda text-center *:m-auto *:h-12 *:w-12">
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
          receive an additional <b
            >{Number($luckyPoolState?.airdrop_amount[0] || 10n) / 2}</b
          > tokens per user.
        </span>
      </p>
    </div>
  {:else}
    <h3 class="h3 !mt-0 text-center">🪂 🎁</h3>
    <div class="!mt-0 text-center text-xl font-bold">Get the Airdrop</div>
    <div class="m-auto !mt-0 flex flex-col content-center">
      <h6 class="h6 mt-5 mb-4 text-center font-bold">
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
    </div>

    <hr
      class="!border-gray/20 mx-[-24px] !border-t-1 !border-dashed {dmChallenge
        ? '!mt-6'
        : '!mt-3'}"
    />
    <form
      class="m-auto !mt-0 flex flex-col content-center"
      on:input={onFormChange}
    >
      <h6 class="h6 mt-5 mb-4 text-center font-bold">
        <span>STEP 2 : Fill-in airdrop code</span>
      </h6>
      <label class="label {!dmChallenge ? 'collapse h-0' : 'visible mb-2'}">
        <span>Airdrop code:</span>
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
          />
          <button
            class="btn absolute top-0 right-0 outline-0"
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

        <span class="text-sm text-pink-500">
          You can get this code from someone participating in <b>Lucky Draw</b>.
        </span>
      </label>
      <label class="label">
        <span>Lucky Code (Optinal):</span>
        <div class="relative">
          <input
            class="input border-gray/10 invalid:input-warning truncate rounded-xl bg-white/20 pr-16 hover:bg-white/90"
            type="text"
            name="luckyCode"
            minlength="6"
            maxlength="6"
            bind:value={luckyCode}
            placeholder="Enter code"
            disabled={submitting}
          />
          <button
            class="btn absolute top-0 right-0 outline-0"
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
    <footer class="m-auto !mt-6">
      <button
        class="btn flex w-full flex-row items-center gap-2 bg-pink-500 text-white"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        {#if submitting}
          <span class=""><IconCircleSpin /></span>
          <span>Processing...</span>
        {:else}
          <span>Claim Now</span>
        {/if}
      </button>
    </footer>
  {/if}
</ModalCard>
