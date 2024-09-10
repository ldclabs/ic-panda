<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import { toastRun } from '$lib/stores/toast'
  import {
    myMessageStateAsync,
    type ChannelInfoEx,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { sleep } from '$src/lib/utils/helper'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelMessages from './ChannelMessages.svelte'
  import ChannelSetting from './ChannelSetting.svelte'

  export let channelId: string

  const toastStore = getToastStore()
  const { canister, id } = ChannelAPI.parseChannelParam(channelId)

  let myState: MyMessageState
  let myInfo: Readable<UserInfo>
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
    const { abort } = toastRun(async (signal: AbortSignal) => {
      if (canister) {
        myState = await myMessageStateAsync()
        myInfo = myState.info as Readable<UserInfo>
        channelInfo = await myState.loadChannelInfo(canister, id)
        openSettings = !$channelInfo._kek
      } else {
        goto('/_/messages')
      }
    }, toastStore)

    return abort
  })
</script>

<div
  class="grid max-h-[calc(100dvh-80px)] grid-rows-[auto_1fr] sm:rounded-tr-2xl"
>
  <header
    class="flex h-[60px] flex-row items-center justify-between gap-2 border-b border-surface-500/30 px-4 py-2"
  >
    <div class="flex flex-row items-center gap-2">
      {#if $channelInfo}
        <Avatar
          initials={$channelInfo.name}
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
