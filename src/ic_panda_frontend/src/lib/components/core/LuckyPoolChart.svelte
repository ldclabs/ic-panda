<script lang="ts">
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type State
  } from '$lib/canisters/luckypool'
  import IconCrown from '$lib/components/icons/IconCrown.svelte'
  import { shortId } from '$lib/utils/auth'
  import { formatNumber, ICPToken, PANDAToken } from '$lib/utils/token'
  import { ProgressBar, Tab, TabGroup, Table } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'

  let tabSet: number = 0
  let luckyPoolAPI: LuckyPoolAPI
  let luckyPoolState: Readable<State | null>
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
      luckyPoolAPI = await luckyPoolAPIAsync()
      luckyPoolState = luckyPoolAPI.stateStore

      while (interval) {
        await new Promise((res) => setTimeout(res, 10000))
        await luckyPoolAPI?.refreshAllState()
      }
    })()

    return () => {
      interval = false
    }
  })

  $: {
    if (luckyPoolAPI) {
      luckyPoolState = luckyPoolAPI.stateStore
      airdropRecords = $luckyPoolState?.latest_airdrop_logs || []
      luckydrawRecords = $luckyPoolState?.latest_luckydraw_logs || []
      highestLuckydrawRecords = $luckyPoolState?.luckiest_luckydraw_logs || []
    }
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
    <div
      class="mt-10 flex w-full flex-row justify-around gap-4 max-sm:flex-col"
    >
      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold">{formatNumber(TotalAmount)}</h3>
        <p class="text-gray/50">Total Amount</p>
      </div>

      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold text-panda">
          {formatNumber(Number($luckyPoolState.total_airdrop / PANDAToken.one))}
        </h3>
        <p class="text-gray/50">
          Airdrop Amount, Count: {Number(
            $luckyPoolState.total_airdrop_count +
              ($luckyPoolState.total_prize_count[0] || 0n)
          )}
        </p>
      </div>

      <div class="flex flex-col items-center">
        <h3 class="h3 text-[28px] font-bold text-panda">
          {formatNumber(
            Number($luckyPoolState.total_luckydraw / PANDAToken.one)
          )}
        </h3>
        <p class="text-gray/50">
          Lucky Draw Amount, Count: {Number(
            $luckyPoolState.total_luckydraw_count
          )}
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
    <TabGroup
      justify="justify-center"
      border="border-none"
      padding="px-2 py-2 md:px-6 md:py-3"
      active="border-b-4 border-primary-500/80"
      hover="hover:bg-primary-500/10"
      class="w-full"
    >
      <Tab bind:group={tabSet} name="AirdropRecords" value={0}>
        Airdrop Records
      </Tab>
      <Tab bind:group={tabSet} name="LuckyDrawRecords" value={1}>
        Lucky Draw Records
      </Tab>
      <Tab bind:group={tabSet} name="MyLuckyDrawRecords" value={2}>
        My Lucky Draw
      </Tab>
      <!-- Tab Panels --->
      <svelte:fragment slot="panel">
        {#if tabSet === 0}
          <Table
            class="-mt-4 mb-8"
            regionHeadCell="bg-white"
            regionBody="*:!border-gray/5"
            regionCell="bg-white !py-3 text-sm text-gray/60"
            source={airdropRecordsSource(airdropRecords)}
          />
        {:else if tabSet === 1}
          <Table
            class="-mt-4 mb-8"
            regionHeadCell="bg-white"
            regionBody="*:!border-gray/5"
            regionCell="bg-white !py-3 text-sm text-gray/60"
            source={luckydrawRecordsSource(luckydrawRecords)}
          />
          {#if highestLuckydrawRecords.length > 0}
            <div class="-mt-4 mb-4 text-center">
              <button
                class="btn m-auto rounded-xl bg-panda/10 font-bold text-panda"
              >
                <span><IconCrown /></span>
                <span>Top 3 Luckiest Draw</span>
              </button>
            </div>
            <Table
              class="mb-8"
              regionHeadCell="bg-white hidden"
              regionBody="*:!border-gray/5"
              regionCell="bg-white !py-3 text-sm text-gray/60"
              source={luckydrawRecordsSource(highestLuckydrawRecords)}
            />
          {/if}
        {:else}
          {#await myLuckydrawRecordsSource() then items}
            <Table
              class="-mt-4 mb-8"
              regionHeadCell="bg-white"
              regionBody="*:!border-gray/5"
              regionCell="bg-white !py-3 text-sm text-gray/60"
              source={items}
            />
          {/await}
        {/if}
      </svelte:fragment>
    </TabGroup>
  {/if}
</div>
