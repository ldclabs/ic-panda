<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import AirdropCard from '$lib/components/core/AirdropCard.svelte'
  import LuckyPoolChart from '$lib/components/core/LuckyPoolChart.svelte'
  import PageFooter from '$lib/components/core/PageFooter.svelte'
  import PrizeCard from '$lib/components/core/PrizeCard.svelte'
  import { authStore } from '$lib/stores/auth'
  import { onMount } from 'svelte'

  onMount(async () => {
    await luckyPoolAPI.refreshAllState()
  })

  $: principal = $authStore.identity.getPrincipal()
</script>

<div class="flex flex-col flex-nowrap content-center items-center px-4">
  {#key principal.toText()}
    <div
      class="flex w-full max-w-4xl flex-col flex-nowrap content-center items-center"
    >
      <div class="mt-8 w-full max-w-[820px]">
        <AirdropCard />
      </div>
      <div class="mt-6 w-full max-w-[820px]">
        <PrizeCard />
      </div>
      <div class="mt-6 w-full max-w-[820px]">
        <LuckyPoolChart />
      </div>
    </div>
  {/key}

  <footer id="page-footer" class="mt-20 flex-none lg:mt-60">
    <PageFooter />
  </footer>
</div>
