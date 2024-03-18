<script lang="ts">
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import IconRefresh from '$lib/components/icons/IconRefresh.svelte'
  import {
    luckyPoolAPIStore,
    LuckyPoolAPI,
    type Captcha,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import { type Readable } from 'svelte/store'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import IconGoldPanda from '$lib/components/icons/IconGoldPanda.svelte'
  import { formatNumber } from '$lib/utils/token'
  import { PANDAToken } from '$lib/utils/token'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import { page } from '$app/stores'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let submitting = false
  let validating = false
  let refreshCaptcha = false
  let luckyPoolAPI: Readable<LuckyPoolAPI>
  let captcha: Captcha
  let captchaCode = ''
  let luckyCode = $page.url.searchParams.get('ref') || ''
  let result: AirdropState

  const modalStore = getModalStore()
  const toastStore = getToastStore()
  const luckyLink = 'https://panda.fans/?ref='

  async function onRefreshCaptcha() {
    if ($luckyPoolAPI) {
      refreshCaptcha = true
      captcha = await $luckyPoolAPI.captcha()
      captchaCode = ''
      refreshCaptcha = false
    }
  }

  async function onFormSubmit() {
    submitting = true
    try {
      result = await $luckyPoolAPI.airdrop({
        challenge: captcha.challenge,
        code: captchaCode,
        lucky_code: luckyCode != '' ? [luckyCode] : []
      })
    } catch (err: any) {
      submitting = false
      const message = err?.message || String(err)
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message
      })
    }
    await $luckyPoolAPI.refreshAllState()
  }

  function onFormChange(e: Event) {
    const form = e.currentTarget as HTMLFormElement
    validating = form.checkValidity()
  }

  function onCheckWallet() {
    parent && parent['onClose']()
    modalStore.trigger({
      type: 'component',
      component: { ref: AccountDetailModal }
    })
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIStore
    await onRefreshCaptcha()
  })
</script>

<ModalCard {parent}>
  {#if result}
    <div class="text-center">
      <div class="text-center text-panda *:m-auto *:h-12 *:w-12">
        <IconCheckbox />
      </div>
      <p class="mt-4">
        You have successfully claimed the airdrop reward, please check your
        wallet.
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
    </div>
    <div
      class="!mt-12 flex flex-row justify-between rounded-lg bg-gray/5 px-4 py-3"
    >
      <div class="flex flex-row">
        <span><IconWallet /></span>
        <span class="ml-2">Wallet</span>
      </div>
      <div class="flex flex-row">
        <span>
          {'+ ' + formatNumber(Number(result.claimed / PANDAToken.one))}
        </span>
        <span class="ml-2 *:mt-[2px] *:h-5 *:w-5"><IconGoldPanda /></span>
      </div>
    </div>
    <div class="!mt-12">
      <button
        class="variant-filled btn btn-lg m-auto block"
        on:click={onCheckWallet}
      >
        Check Wallet
      </button>
    </div>
  {:else}
    <h6 class="h6">Free PANDA Tokens Airdrop Rules</h6>
    <ol class="list text-gray/50">
      <li>
        <span class="variant-soft-primary badge-icon p-4">1</span>
        <span class="flex-auto">
          Each new user can claim 100 tokens, or 150 tokens with a valid lucky
          code.
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
          receive an additional 50 tokens.
        </span>
      </li>
    </ol>
    <hr class="!border-t-1 !border-gray/10" />
    <div class="relative">
      {#if captcha}
        <img class="m-auto w-60" src={captcha.img_base64} alt="Captcha" />
        <button
          class="btn btn-icon absolute right-3 top-3 hover:*:scale-110 max-md:right-0 {refreshCaptcha
            ? 'animate-spin'
            : ''}"
          on:click={onRefreshCaptcha}
          disabled={refreshCaptcha}
        >
          <IconRefresh />
        </button>
      {:else}
        <div class="placeholder m-auto h-16 w-60 animate-pulse rounded-none" />
      {/if}
    </div>
    <form class="flex flex-col gap-4" on:change={onFormChange}>
      <div
        class="input-group input-group-divider grid-cols-[auto_1fr_auto] bg-gray/5"
      >
        <div class="input-group-shim bg-gray/5">Captcha Code</div>
        <input
          class="input rounded-none invalid:input-warning hover:bg-white/90"
          type="text"
          name="captchaCode"
          minlength="4"
          maxlength="4"
          bind:value={captchaCode}
          placeholder="Enter code"
          disabled={!captcha || submitting}
          required
        />
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
        class="variant-filled-primary btn w-full text-white"
        disabled={submitting || !validating}
        on:click={onFormSubmit}
      >
        Claim
      </button>
    </footer>
  {/if}
</ModalCard>
