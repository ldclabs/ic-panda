<script lang="ts">
  import {
    luckyPoolAPI,
    type PrizeClaimLog,
    type PrizeOutput
  } from '$lib/canisters/luckypool'
  import IconArrowDownFill from '$lib/components/icons/IconArrowDownFill.svelte'
  import IconArrowUpFill from '$lib/components/icons/IconArrowUpFill.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import MemoDetail from '$lib/components/ui/MemoDetail.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { errMessage } from '$lib/types/result'
  import { formatNumber, PANDAToken } from '$lib/utils/token'
  import { getToastStore } from '$lib/ui/stores'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

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
    prizeClaimLogsRes = prizeClaimLogs()
    prizeIssueLogsRes = prizeIssueLogs()
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Prize History</div>
  <div class="!mt-2">
    <div class="mx-6 flex justify-center" role="tablist">
      {#each ['Receive', 'Send'] as label, index}
        <button
          class="border-b-4 px-6 py-2 {tabSet === index
            ? 'border-panda font-semibold'
            : 'text-gray/50 border-transparent'}"
          role="tab"
          aria-selected={tabSet === index}
          on:click={() => (tabSet = index)}>{label}</button
        >
      {/each}
    </div>
    <div class="mt-3">
      <div class="space-y-3">
        {#if luckyPoolAPI && tabSet === 0}
          {#await prizeClaimLogsRes}
            <div class="m-auto w-fit text-center"><Loading /></div>
          {:then items}
            {#each items as item}
              <div class="bg-gray/5 rounded-lg p-3">
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
                    <span class="text-panda font-semibold">
                      {formatNumber(
                        Number(item.amount) / Number(PANDAToken.one)
                      ) + ' PANDA'}
                    </span>
                  </div>
                </div>
                <div class="mt-2 pl-8 text-sm">
                  <b
                    >From: {item.prize.name.length > 0
                      ? item.prize.name[0]
                      : '-'}</b
                  >
                </div>
                <MemoDetail memo={item.prize.memo[0] || null} />
              </div>
            {/each}
          {/await}
        {:else if luckyPoolAPI && tabSet === 1}
          {#await prizeIssueLogsRes}
            <div class="m-auto w-fit text-center"><Loading /></div>
          {:then items}
            {#each items as item}
              {@const link =
                item.code.length > 0
                  ? `${APP_ORIGIN}?prize=${item.code[0]}`
                  : '-'}
              <div class="bg-gray/5 rounded-lg p-3">
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
                    <span class="text-panda font-semibold">
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
                    <p class="text-gray/50 w-full truncate">{link}</p>
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
    </div>
  </div>
</ModalCard>
