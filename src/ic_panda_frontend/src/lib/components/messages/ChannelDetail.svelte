<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import { toastRun } from '$lib/stores/toast'
  import {
    type ChannelInfoEx,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { sleep } from '$src/lib/utils/helper'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import { getContext, onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelMessages from './ChannelMessages.svelte'
  import ChannelSetting from './ChannelSetting.svelte'

  export let channelId: string
  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>

  const toastStore = getToastStore()
  const { canister, id } = ChannelAPI.parseChannelParam(channelId)
  const onChatBack = getContext('onChatBack') as () => void

  let channelInfo: Readable<ChannelInfoEx>

  let openSettings = false
  let switching = false
  async function onClickChannelSetting() {
    if (!canister) return
    switching = true
    await Promise.all([
      async () => {
        // load latest channel info
        channelInfo = await myState.loadChannelInfo(canister, id)
      },
      sleep(200)
    ])
    openSettings = !openSettings
    switching = false
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(
      async (signal: AbortSignal) => {
        if (canister) {
          channelInfo = await myState.loadChannelInfo(canister, id)
          openSettings = !$channelInfo._kek
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

<div class="grid grid-rows-[auto_1fr]">
  <header
    class="flex h-[60px] flex-row items-center justify-between gap-2 border-b border-surface-500/30 px-4 py-2"
  >
    <div class="md:hidden">
      <button class="btn -ml-6" on:click={onChatBack}
        ><IconArrowLeftSLine /></button
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      {#if $channelInfo}
        <Avatar
          initials={$channelInfo.name}
          src={$channelInfo.image}
          background="bg-panda"
          fill="fill-white"
          width="w-8"
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
      class="btn-icon"
      title="Channel settings"
      disabled={switching}
      on:click={onClickChannelSetting}
      ><span>
        {#if openSettings}
          <IconClose />
        {:else}
          <IconMoreFill />
        {/if}
      </span></button
    >
  </header>
  {#if $channelInfo}
    {#if openSettings}
      <ChannelSetting {myState} {myInfo} channelInfo={$channelInfo} />
    {:else}
      <ChannelMessages {myState} {myInfo} channelInfo={$channelInfo} />
    {/if}
  {:else}
    <div class="m-auto size-24 rounded-full *:size-24">
      <IconPanda />
    </div>
  {/if}
</div>
