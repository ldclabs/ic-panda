<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type AirdropState
  } from '$lib/canisters/luckypool'
  import IconCheckbox from '$lib/components/icons/IconCheckbox.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { decodePrize } from '$lib/types/prize'
  import { errMessage } from '$lib/types/result'
  import { PANDAToken, formatNumber } from '$lib/utils/token'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  const modalStore = getModalStore()

  let submitting = false
  let cryptogram = $page.url.searchParams.get('prize') || ''
  let validating = decodePrize(cryptogram) != null
  let luckyPoolAPI: LuckyPoolAPI
  let result: AirdropState

  const toastStore = getToastStore()

  async function onFormSubmit() {
    submitting = true
    try {
      result = await luckyPoolAPI.prize(cryptogram)
      // Remove the prize query parameter from the URL
      if ($page.url.searchParams.get('prize')) {
        const query = $page.url.searchParams
        query.delete('prize')
        goto(`?${query.toString()}`)
      }
    } catch (err: any) {
      submitting = false

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
    const form = e.currentTarget as HTMLFormElement
    validating = form.checkValidity()
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
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
              Number(
                result.claimable - ($modalStore[0]?.meta.claimableAmount || 0n)
              ) / Number(PANDAToken.one)
            )}</b
          > PANDA tokens.
        </span>
      </p>
      <p class="mt-4 text-left">
        Follow the <a
          title="Follow on Twitter"
          class="text-panda underline"
          href="https://twitter.com/ICPandaDAO"
          target="_blank">ICPanda Twitter</a
        >, or join the
        <a
          title="Join the Community"
          class="text-panda underline"
          href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
          target="_blank">ICPanda Community</a
        >, to get the latest prize messages in time.
      </p>
    </div>
  {:else}
    <h6 class="h6">Prize Rules</h6>
    <ol class="list text-gray/50">
      <li>
        <span class="variant-soft-primary badge-icon p-4">1</span>
        <span class="flex-auto">
          Have your own <b>lucky code</b> through airdrop.
        </span>
      </li>
      <li>
        <span class="variant-soft-primary badge-icon p-4">2</span>
        <span class="flex-auto">
          Have at least <b>10 PANDA</b> tokens in your wallet.
        </span>
      </li>
      <li>
        <span class="variant-soft-primary badge-icon p-4">3</span>
        <span class="flex-auto">
          Each reward has a limit on quantity, first come first serve.
        </span>
      </li>
    </ol>
    <hr class="!border-t-1 !border-gray/10" />
    <form class="flex flex-col gap-4" on:input={onFormChange}>
      <div
        ><p>
          Follow the <a
            title="Follow on Twitter"
            class="text-panda underline"
            href="https://twitter.com/ICPandaDAO"
            target="_blank">ICPanda Twitter</a
          >, or join the
          <a
            title="Join the Community"
            class="text-panda underline"
            href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
            target="_blank">ICPanda Community</a
          >, to get the latest prize messages in time.
        </p>
      </div>
      <div
        class="input-group input-group-divider grid-cols-[auto_1fr_auto] bg-gray/5"
      >
        <div class="input-group-shim bg-gray/5">Cryptogram</div>
        <input
          class="input rounded-none invalid:input-warning hover:bg-white/90"
          type="text"
          name="cryptogram"
          minlength="20"
          maxlength="50"
          bind:value={cryptogram}
          placeholder="Enter code"
          disabled={submitting}
          required
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
