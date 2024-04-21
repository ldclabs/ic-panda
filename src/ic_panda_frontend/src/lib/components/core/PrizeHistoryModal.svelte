<script lang="ts">
  import {
    LuckyPoolAPI,
    luckyPoolAPIAsync,
    type PrizeClaimLog,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import IconArrowDownFill from '$lib/components/icons/IconArrowDownFill.svelte'
  import IconArrowUpFill from '$lib/components/icons/IconArrowUpFill.svelte'
  import MemoDetail from '$lib/components/ui/MemoDetail.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { formatNumber, PANDAToken } from '$lib/utils/token'
  import { getToastStore, Tab, TabGroup } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  let luckyPoolAPI: LuckyPoolAPI
  let tabSet: number = 0
  let prizeClaimLogsRes: Promise<Array<PrizeClaimLog>>
  let prizeIssueLogsRes: Promise<Array<PrizeOutput>>

  const toastStore = getToastStore()

  async function prizeClaimLogs(): Promise<Array<PrizeClaimLog>> {
    try {
      const res = await luckyPoolAPI.prizeClaimLogs(0n, 100n)
      return res
    } catch (err: any) {
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
    return []
  }

  async function prizeIssueLogs(): Promise<Array<PrizeOutput>> {
    try {
      const res = await luckyPoolAPI.prizeIssueLogs(0n)
      return res
    } catch (err: any) {
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
    return []
  }

  onMount(async () => {
    luckyPoolAPI = await luckyPoolAPIAsync()
    prizeClaimLogsRes = prizeClaimLogs()
    prizeIssueLogsRes = prizeIssueLogs()
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Prize History</div>
  <TabGroup
    justify="justify-center"
    border="border-none"
    padding="px-0 py-2 mx-6"
    active="border-b-4 border-primary-500/80"
    hover="hover:bg-transparent"
    class="!mt-2"
  >
    <Tab bind:group={tabSet} name="Receive" value={0}>
      <span>Receive</span>
    </Tab>
    <Tab bind:group={tabSet} name="Send" value={1}>
      <span>Send</span>
    </Tab>
    <!-- Tab Panels --->
    <svelte:fragment slot="panel">
      <div class="space-y-3">
        {#if luckyPoolAPI && tabSet === 0}
          {#await prizeClaimLogsRes}
            <div class="">Loading</div>
          {:then items}
            {#each items as item}
              <div class="rounded-lg bg-gray/5 p-3">
                <div class="flex flex-row items-center justify-between">
                  <div class="flex flex-row items-center gap-2">
                    <span class="text-panda"><IconArrowDownFill /></span>
                    <span class="text-sm">
                      {new Date(
                        Number(item.claimed_at * 1000n)
                      ).toLocaleString()}
                    </span>
                  </div>
                  <div class="">
                    <span class="font-semibold text-panda">
                      {formatNumber(
                        Number(item.amount) / Number(PANDAToken.one)
                      ) + ' PANDA'}
                    </span>
                  </div>
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>From:</span>
                  <span>
                    {item.prize.name.length > 0 ? item.prize.name[0] : ''}
                  </span>
                </div>
                <MemoDetail memo={item.prize.memo[0] || null} />
              </div>
            {/each}
          {/await}
        {:else if luckyPoolAPI && tabSet === 1}
          {#await prizeIssueLogsRes}
            <div class="">Loading</div>
          {:then items}
            {#each items as item}
              {@const link =
                item.code.length > 0
                  ? `${APP_ORIGIN}?prize=${item.code[0]}`
                  : '-'}
              <div class="rounded-lg bg-gray/5 p-3">
                <div class="flex flex-row items-center justify-between">
                  <div class="flex flex-row items-center gap-2">
                    <span class=""><IconArrowUpFill /></span>
                    <span class="text-sm">
                      {new Date(
                        Number(item.issued_at * 1000n)
                      ).toLocaleString()}
                    </span>
                  </div>
                  <div class="">
                    <span class="font-semibold text-panda">
                      {'-' +
                        formatNumber(
                          Number(
                            item.amount +
                              item.fee -
                              item.sys_subsidy -
                              item.refund_amount
                          ) / Number(PANDAToken.one)
                        ) +
                        ' PANDA'}
                    </span>
                  </div>
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Total amount:</span>
                  <span
                    >{'-' +
                      formatNumber(
                        Number(item.amount) / Number(PANDAToken.one)
                      ) +
                      ' PANDA'}</span
                  >
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Prize fee:</span>
                  <span
                    >{'-' +
                      formatNumber(Number(item.fee) / Number(PANDAToken.one)) +
                      ' PANDA'}</span
                  >
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Subsidy:</span>
                  <span
                    >{'+' +
                      formatNumber(
                        Number(item.sys_subsidy) / Number(PANDAToken.one)
                      ) +
                      ' PANDA'}</span
                  >
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Distribution:</span>
                  <span>
                    {item.kind == 0 ? 'Equal' : 'Random'}
                  </span>
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Quantity:</span>
                  <span>
                    {`${item.filled}/${item.quantity} Claimed`}
                  </span>
                </div>
                {#if item.ended_at > 0n}
                  <div
                    class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                  >
                    <span>End at:</span>
                    <span>
                      {new Date(Number(item.ended_at * 1000n)).toLocaleString()}
                    </span>
                  </div>
                {:else}
                  <div
                    class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                  >
                    <span>Expire at:</span>
                    <span>
                      {new Date(
                        Number((item.issued_at + item.expire) * 1000n)
                      ).toLocaleString()}
                    </span>
                  </div>
                {/if}
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Prize Link:</span>
                  <div class="flex w-[220px] flex-row items-center gap-1">
                    <p class="w-full truncate text-gray/50">{link}</p>
                    {#if link !== '-'}
                      <TextClipboardButton textValue={link} />
                    {/if}
                  </div>
                </div>
                <div
                  class="mt-2 flex flex-row items-center justify-between gap-2 pl-8 text-sm"
                >
                  <span>Refund:</span>
                  <span
                    >{item.refund_amount > 0n
                      ? '+' +
                        formatNumber(
                          Number(item.refund_amount) / Number(PANDAToken.one)
                        ) +
                        ' PANDA'
                      : '-'}</span
                  >
                </div>
                <MemoDetail memo={item.memo[0] || null} />
              </div>
            {/each}
          {/await}
        {/if}
      </div>
    </svelte:fragment>
  </TabGroup>
</ModalCard>
