<script lang="ts">
  import { type PrizeOutput } from '$lib/canisters/luckypool'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconCloseCircleLine from '$lib/components/icons/IconCloseCircleLine.svelte'
  import IconDice from '$lib/components/icons/IconDice.svelte'
  import IconEqualizer from '$lib/components/icons/IconEqualizer.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { mapToObj } from '$lib/utils/fetcher'
  import { decodeCBOR } from '@ldclabs/cose-ts/utils'
  import { ProgressBar } from '@skeletonlabs/skeleton'

  export let prizeInfo: PrizeOutput
  export let claimPrize: () => Promise<void>
  export let close: () => void

  let submitting = false
  let canClaim = false
  let detail: { message: string; link: string } | null

  async function claimHandler(e: Event) {
    e.preventDefault()

    submitting = true
    await claimPrize()
    submitting = false
    canClaim = false
  }

  $: principal = $authStore.identity.getPrincipal()
  $: canClaim =
    prizeInfo.filled < prizeInfo.quantity && prizeInfo.ended_at == 0n
  $: detail = prizeInfo.memo[0]
    ? mapToObj(decodeCBOR(prizeInfo.memo[0] as Uint8Array))
    : null
</script>

<section class="absolute left-0 right-0 top-0 !m-0 rounded-3xl">
  <div
    class="relative bg-[url('/_assets/images/prize-bg.webp')] bg-[length:100%_auto] bg-no-repeat"
  >
    <div
      class="flex flex-row items-center justify-center gap-1 pt-12 text-orange-600"
    >
      {#if prizeInfo.kind == 0}
        <span class=""><IconEqualizer /></span>
        <span class="">Equal distribution</span>
      {:else}
        <span class=""><IconDice /></span>
        <span class="">Random distribution</span>
      {/if}
    </div>
    <div class="pt-[198px] text-center text-white">
      <b class="text-2xl">{prizeInfo.name[0] || 'Good Luck To You'}</b>
    </div>
    {#if detail}
      <div class="m-auto w-11/12 pt-2 text-center text-white/80">
        {detail.message}
      </div>
      {#if detail.link}
        <a
          class="m-auto block w-fit max-w-[80%] truncate pt-2 text-center text-white/80 underline"
          href={detail.link}
          target="_blank"
        >
          {detail.link}
        </a>
      {/if}
    {/if}
    <div class="pt-10 text-center font-semibold text-white/90">
      <span>{`${prizeInfo.filled} / ${prizeInfo.quantity}`}</span>
    </div>
    <div class="m-auto w-11/12 pt-2">
      <ProgressBar
        label="PANDA Prize Claiming Progress"
        height="h-3"
        width="w-10/12"
        meter="bg-white/80"
        track="bg-white/20"
        value={prizeInfo.filled}
        max={prizeInfo.quantity}
      />
    </div>
    <div class="pb-8 pt-4 text-center text-white/90">
      {#if principal.isAnonymous()}
        <button
          class="btn m-auto flex w-6/12 flex-row items-center gap-2 bg-white text-orange-600"
          on:click={() => signIn()}
        >
          <span>Login</span>
        </button>
      {:else}
        <button
          class="btn m-auto flex w-6/12 flex-row items-center gap-2 {canClaim
            ? 'bg-white text-orange-600'
            : 'bg-white/20 text-white/60'}"
          disabled={submitting || !canClaim}
          on:click={claimHandler}
        >
          {#if prizeInfo.ended_at > 0n}
            <span>Expired</span>
          {:else if prizeInfo.filled == prizeInfo.quantity}
            <span>Fully Claimed</span>
          {:else if submitting}
            <span class=""><IconCircleSpin /></span>
            <span>Processing...</span>
          {:else}
            <span>Claim Now</span>
          {/if}
        </button>
      {/if}
    </div>
  </div>
  <button
    class="z-1 btn btn-icon m-auto block w-fit translate-y-[40px] text-white *:scale-125 hover:scale-110"
    on:click={close}
  >
    <IconCloseCircleLine />
  </button>
</section>

<style>
  section {
    background: linear-gradient(142deg, #fcd34d 1%, #ef4444 94%);
    box-shadow: inset -11.78px -11.78px 24.32px 0px #d33327;
  }
</style>
