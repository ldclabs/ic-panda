<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { type ChannelInfoEx, type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import type { Principal } from '@dfinity/principal'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import { getContext, onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelMessages from './ChannelMessages.svelte'
  import ChannelSetting from './ChannelSetting.svelte'

  export let channelId: { canister: Principal; id: number }
  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>

  const toastStore = getToastStore()
  const { canister, id } = channelId
  const onChatBack = getContext('onChatBack') as () => void

  let channelInfo: Readable<ChannelInfoEx>
  let isLoading = true
  let openMessages = true
  let switching = false
  async function onClickChannelSetting() {
    if (!canister) return
    switching = true
    await Promise.all([
      async () => {
        // load latest channel info
        channelInfo = await myState.loadChannelInfo(canister, id)
      },
      sleep(100)
    ])
    openMessages = !openMessages
    switching = false
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(
      async (signal: AbortSignal) => {
        if (canister) {
          channelInfo = await myState.loadChannelInfo(canister, id)
          openMessages = !!$channelInfo?._kek
          if (openMessages) {
            try {
              // try to decrypt channel DEK
              await myState.decryptChannelDEK($channelInfo)
            } catch (_e) {
              openMessages = false
            }
          }
          isLoading = false
          return channelInfo
        }
        return null
      },
      toastStore
    )

    onfinally((channel) => {
      if (!channel) {
        goto('/_/messages')
      }
    })

    return abort
  })
</script>

<div class="grid h-[calc(100dvh-60px)] grid-rows-[auto_1fr] md:h-dvh">
  <header
    class="flex h-[60px] flex-row items-center justify-between gap-2 border-b border-surface-500/20 px-0 py-2 md:px-4"
  >
    <div class="md:hidden">
      <button
        class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
        on:click={onChatBack}><IconArrowLeftSLine /></button
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      {#if $channelInfo}
        <Avatar
          initials={$channelInfo.name}
          src={$channelInfo.image}
          background={$channelInfo.image ? '' : 'bg-panda'}
          fill="fill-white"
          width="w-10"
        />
        <span class="flex-1 text-start">
          {$channelInfo.name +
            ' (' +
            ($channelInfo.managers.length + $channelInfo.members.length) +
            ')'}
        </span>
      {:else}
        <Avatar
          initials=""
          background="bg-panda"
          fill="fill-white"
          width="w-8"
        />
      {/if}
    </div>
    <button
      type="button"
      class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
      title="Channel settings"
      disabled={switching}
      on:click={onClickChannelSetting}
    >
      {#if openMessages && $channelInfo?.ecdh_request.length > 0}
        <span class="badge-icon z-10 size-2 bg-error-500"></span>
      {/if}
      <span>
        {#if openMessages}
          <IconMoreFill />
        {:else}
          <IconClose />
        {/if}
      </span>
    </button>
  </header>
  {#if isLoading}
    <Loading />
  {:else if $channelInfo}
    {#if openMessages}
      <ChannelMessages {myState} {myInfo} channelInfo={$channelInfo} />
    {:else}
      <ChannelSetting {myState} {myInfo} channelInfo={$channelInfo} />
    {/if}
  {:else}
    <div class="m-auto size-24 rounded-full *:size-24">
      <IconPanda />
    </div>
  {/if}
</div>
