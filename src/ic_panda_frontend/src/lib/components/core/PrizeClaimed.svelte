<script lang="ts">
  import {
    type ClaimPrizeOutput,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import IconCloseCircleLine from '$lib/components/icons/IconCloseCircleLine.svelte'
  import IconOpenChat from '$lib/components/icons/IconOpenChat.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { PANDAToken, formatNumber } from '$lib/utils/token'

  export let prizeInfo: PrizeOutput
  export let result: ClaimPrizeOutput
  export let close: () => void
</script>

<section class="absolute left-0 right-0 top-0 !m-0 rounded-3xl">
  <div class="prize-header rounded-3xl p-4">
    <div class="h-44 rounded-2xl bg-white pt-6 text-center">
      <p class="text-sm font-semibold">
        {prizeInfo.name[0]
          ? 'Prize From ' + prizeInfo.name[0]
          : 'Good Luck To You'}
      </p>
      <p class="text-4xl font-bold">
        {formatNumber(Number(result.claimed) / Number(PANDAToken.one))}
        <span class="text-sm text-gray/50">PANDA</span>
      </p>
      <p class="text-sm text-gray/50">
        <span>
          Current avg amount:
          {formatNumber(Number(result.average) / Number(PANDAToken.one))}
        </span>
        <span>
          Total:
          {formatNumber(Number(prizeInfo?.amount) / Number(PANDAToken.one))}
        </span>
      </p>
    </div>
  </div>
  <div
    class="mt-[-132px] bg-[url('/_assets/images/prize-claimed-bg.webp')] bg-[length:100%_auto] bg-no-repeat"
  >
    <div class="m-auto w-72 pt-[210px] text-center text-white sm:pt-[240px]">
      <span class="text-lg font-semibold">
        The more lucky balance you hold, the bigger prize you grab.
      </span>
    </div>
    <div
      class="flex flex-row justify-center gap-2 pb-8 pt-16 text-white sm:pt-12"
    >
      <a
        type="button"
        title="Follow on Twitter"
        class="btn btn-sm rounded-xl border-[1px] border-white/50"
        href="https://twitter.com/ICPandaDAO"
        target="_blank"
      >
        <span><IconX /></span>
        <span class="text-left">ICPanda DAO</span>
      </a>
      <a
        type="button"
        title="Join the Community"
        class="btn btn-sm rounded-xl border-[1px] border-white/50"
        href="https://oc.app/community/dqcvf-haaaa-aaaar-a5uqq-cai"
        target="_blank"
      >
        <span><IconOpenChat /></span>
        <span class="text-left">ICPanda Community</span>
      </a>
    </div>
  </div>
  <button
    class="z-1 btn btn-icon m-auto block w-fit translate-y-8 text-white *:scale-125 hover:scale-110"
    on:click={close}
  >
    <IconCloseCircleLine />
  </button>
</section>

<style>
  div.prize-header {
    background: linear-gradient(133deg, #fcd34d 3%, #ef4444 124%);
  }
</style>
