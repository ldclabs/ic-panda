<script lang="ts">
  import { luckyPoolAPI } from '$lib/canisters/luckypool'
  import IconCrown from '$lib/components/icons/IconCrown.svelte'
  import DataTable from '$lib/components/ui/DataTable.svelte'
  import ProgressBar from '$lib/components/ui/ProgressBar.svelte'
  import { authStore } from '$lib/stores/auth'
  import { shortId } from '$lib/utils/helper'
  import { formatNumber, ICPToken, PANDAToken } from '$lib/utils/token'
  import { isActive } from '$lib/utils/window'
  import { onMount } from 'svelte'

  let tabSet: number = 0
  let luckyPoolState = luckyPoolAPI.stateStore
  let airdropRecords: any[]
  let luckydrawRecords: any[]
  let highestLuckydrawRecords: any[]

  const TotalAmount = 500000000 // in PANDA tokens

  function airdropRecordsSource(items: any[]) {
    return {
      head: ['Time', 'ID', 'User', '$PANDA'],
      body: items.map((item) => [
        new Date(Number(item.ts) * 1000).toLocaleString(),
        String(item.id),
        shortId(item.caller.toString()),
        item.amount > 0
          ? formatNumber(Number(item.amount) / Number(PANDAToken.one))
          : '-'
      ])
    }
  }

  function luckydrawRecordsSource(items: any[]) {
    return {
      head: ['Time', 'ID', 'User', '$PANDA', '$ICP Cost', 'Random No.'],
      body: items.map((item) => [
        new Date(Number(item.ts) * 1000).toLocaleString(),
        String(item.id),
        shortId(item.caller.toString()),
        formatNumber(Number(item.amount) / Number(PANDAToken.one)),
        formatNumber(Number(item.icp_amount) / Number(ICPToken.one)),
        String(item.random)
      ])
    }
  }

  async function myLuckydrawRecordsSource() {
    const items = await luckyPoolAPI.myLuckydrawLogs()
    return {
      head: ['Time', 'ID', 'User', '$PANDA', '$ICP Cost', 'Random No.'],
      body: items.map((item) => [
        new Date(Number(item.ts) * 1000).toLocaleString(),
        String(item.id),
        shortId(item.caller.toString()),
        formatNumber(Number(item.amount) / Number(PANDAToken.one)),
        formatNumber(Number(item.icp_amount) / Number(ICPToken.one)),
        String(item.random)
      ])
    }
  }

  onMount(() => {
    let interval = true
    ;(async () => {
      const ms = principal.isAnonymous() ? 20000 : 10000

      while (interval) {
        await new Promise((res) => setTimeout(res, ms))
        if (isActive()) {
          await luckyPoolAPI.refreshAllState()
        }
      }
    })()

    return () => {
      interval = false
    }
  })

  $: principal = $authStore.identity.getPrincipal()
  $: {
    airdropRecords = $luckyPoolState?.latest_airdrop_logs || []
    luckydrawRecords = $luckyPoolState?.latest_luckydraw_logs || []
    highestLuckydrawRecords = $luckyPoolState?.luckiest_luckydraw_logs || []
  }
</script>

<div
  class="card flex flex-col items-center rounded-2xl rounded-b-none bg-white p-10"
>
  <h3 class="h3 text-center font-black">Lucky Pool Consumption Progress</h3>
  {#if $luckyPoolState}
    {@const consumedAmount = Number(
      ($luckyPoolState.total_luckydraw + $luckyPoolState.total_airdrop) /
        PANDAToken.one
    )}
    {@const percent =
      String(Math.round((consumedAmount * 100) / TotalAmount)) + '%'}
    <div class="mt-4 flex w-full flex-row justify-around gap-2 max-sm:flex-col">
      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold text-panda">
          <span class="text-sm font-normal text-gray/50">Total:</span>
          {formatNumber(Number($luckyPoolState.total_airdrop / PANDAToken.one))}
        </h3>
        <p class="text-sm text-gray/50">
          Airdrop Count: {Number($luckyPoolState.total_airdrop_count)}
        </p>
      </div>

      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold text-panda">
          <span class="text-sm font-normal text-gray/50">Total:</span>
          {formatNumber(
            Number(($luckyPoolState.total_prize[0] || 0n) / PANDAToken.one)
          )}
        </h3>
        <p class="text-sm text-gray/50">
          Prizes Count: {Number($luckyPoolState.total_prizes_count[0] || 0n)},
          Claim Count: {Number($luckyPoolState.total_prize_count[0] || 0n)}
        </p>
      </div>

      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold text-panda">
          <span class="text-sm font-normal text-gray/50">Total:</span>
          {formatNumber(
            Number($luckyPoolState.total_luckydraw / PANDAToken.one)
          )}
        </h3>
        <p class="text-sm text-gray/50">
          Lucky Draw Count: {Number($luckyPoolState.total_luckydraw_count)}
        </p>
      </div>
    </div>
    <div class="relative mt-8 w-full">
      <ProgressBar
        label="Lucky Pool Consumption Progress"
        height="h-4"
        meter="bg-panda"
        track="bg-gray/10"
        value={consumedAmount}
        max={TotalAmount}
      />
      <div
        class="btn btn-sm absolute -top-2 translate-x-[-28px] border-2 border-panda bg-white py-[2px] text-base font-bold text-panda"
        style:left={percent}
      >
        {percent}
      </div>
    </div>
  {/if}
</div>

<div
  class="card mt-1 flex flex-col items-center rounded-2xl rounded-t-none bg-white px-10 py-4"
>
  {#if $luckyPoolState}
    <div class="w-full">
      <div class="mb-4 flex justify-center overflow-x-auto" role="tablist">
        {#each ['Airdrop Records', 'Lucky Draw Records', 'My Lucky Draw'] as label, index}
          <button
            class="border-b-4 px-2 py-2 text-sm transition md:px-6 md:py-3 {tabSet ===
            index
              ? 'border-panda text-black'
              : 'border-transparent text-gray/50 hover:bg-panda/10'}"
            role="tab"
            aria-selected={tabSet === index}
            on:click={() => (tabSet = index)}
          >{label}</button>
        {/each}
      </div>

      {#if tabSet === 0}
        <DataTable class="mb-8" source={airdropRecordsSource(airdropRecords)} />
      {:else if tabSet === 1}
        <DataTable class="mb-8" source={luckydrawRecordsSource(luckydrawRecords)} />
        {#if highestLuckydrawRecords.length > 0}
          <div class="mb-4 text-center">
            <button class="btn m-auto rounded-xl bg-panda/10 font-bold text-panda">
              <span><IconCrown /></span>
              <span>Top 3 Luckiest Draw</span>
            </button>
          </div>
          <DataTable
            class="mb-8"
            hideHead={true}
            source={luckydrawRecordsSource(highestLuckydrawRecords)}
          />
        {/if}
      {:else}
        {#await myLuckydrawRecordsSource() then items}
          <DataTable class="mb-8" source={items} />
        {/await}
      {/if}
    </div>
  {/if}
</div>
