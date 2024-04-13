<script lang="ts">
  import { page } from '$app/stores'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { XAuth } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { errMessage } from '$lib/types/result'
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
  const luckyLink = 'https://panda.fans/?ref='

  async function onFormSubmit() {
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
    <div class="text-center">
      <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
        <IconCheckbox />
      </div>
      <p class="mt-4">
        <span>
          You have successfully claimed <b
            >{formatNumber(
              Number(result.claimable) / Number(PANDAToken.one)
            )}</b
          > PANDA tokens.
        </span>
      </p>
      <h4 class="h4 mt-4">
        <span>Your lucky code:</span>
        <span class="text-panda">{result.lucky_code[0]}</span>
        <TextClipboardButton textValue={result.lucky_code[0] || ''} />
      </h4>
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
    <h6 class="h6">Free PANDA Tokens Airdrop Rules</h6>
    <ol class="list text-gray/50">
      <li>
        <span class="variant-soft-primary badge-icon p-4">1</span>
        <span class="flex-auto">
          Each new user can claim {defaultClaimable} tokens, or {defaultClaimable +
            defaultClaimable / 2} tokens with a valid lucky code.
        </span>
      </li>
      <li>
        <span class="variant-soft-primary badge-icon p-4">2</span>
        <span class="flex-auto">
          Upon claiming, you'll receive your own lucky code to share with
          others.
        </span>
      </li>
      <li>
        <span class="variant-soft-primary badge-icon p-4">3</span>
        <span class="flex-auto">
          When a new user claims the airdrop using your lucky code, you'll also
          receive an additional {defaultClaimable / 2} tokens.
        </span>
      </li>
    </ol>
    <hr class="!border-t-1 !border-gray/10" />
    <form class="flex flex-col gap-4" on:input={onFormChange}>
      <div class="flex flex-col items-center gap-2">
        <button
          class="variant-filled btn w-full {challenge != '' ? 'bg-panda' : ''}"
          disabled={oAuthSubmitting || submitting}
          on:click={xAuth}
        >
          <span class="">Challenge by Twitter OAuth</span>
          {#if challenge != ''}
            <span class=""><IconCheckbox /></span>
          {:else}
            <span><IconX /></span>
          {/if}
        </button>
        <button
          class="btn text-gray/50"
          disabled={oAuthSubmitting || submitting || challenge != ''}
          on:click={() => {
            dmChallenge = !dmChallenge
          }}
        >
          <span>Or</span>
          <span class=" underline underline-offset-4"
            >Request challenge code from US</span
          >
        </button>
      </div>
      <div
        class="flex flex-col gap-2 {!dmChallenge ? 'collapse h-0' : 'visible'}"
      >
        <p>
          Follow the <a
            title="Follow on Twitter"
            class="text-panda underline"
            href="https://twitter.com/ICPandaDAO"
            target="_blank">ICPanda Twitter</a
          >
          and DM us your <b>Principal ID</b>👇 to get the airdrop
          <b>Challenge Code</b> for you:
        </p>
        <h5 class="h5 my-2 flex flex-row content-center items-center gap-1">
          <p class="truncate text-panda">{principal.toString()}</p>
          <TextClipboardButton textValue={principal.toString()} />
        </h5>
        <p>
          You can also obtain airdrop and lucky code through a <b>Lucky Draw</b
          >.<br />
          You can only exchange for the challenge code once. We will not respond
          to multiple requests from you.
        </p>
        <div
          class="input-group input-group-divider grid-cols-[auto_1fr_auto] bg-gray/5"
        >
          <div class="input-group-shim bg-gray/5">Challenge Code</div>
          <input
            class="input rounded-none invalid:input-warning hover:bg-white/90"
            type="text"
            name="cryptogram"
            minlength="20"
            maxlength="50"
            bind:value={cryptogram}
            placeholder="Enter code"
            disabled={submitting}
          />
        </div>
      </div>
      <div
        class="input-group input-group-divider grid-cols-[auto_1fr_auto] !bg-gray/5"
      >
        <div class="input-group-shim !bg-gray/5">Lucky Code (Optional)</div>
        <input
          class="input rounded-none text-panda invalid:input-warning hover:bg-white/90"
          type="text"
          name="luckyCode"
          minlength="6"
          maxlength="6"
          bind:value={luckyCode}
          placeholder="Enter code"
          disabled={submitting}
        />
      </div>
    </form>
    <footer class="">
      <button
        class="variant-filled-primary btn flex w-full flex-row items-center gap-2 text-white"
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
