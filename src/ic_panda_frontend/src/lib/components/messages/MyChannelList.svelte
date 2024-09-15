<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconNotificationOffLine from '$lib/components/icons/IconNotificationOffLine.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { isActive } from '$lib/utils/window'
  import {
    type ChannelBasicInfoEx,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { readable, type Readable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'

  export let myState: MyMessageState

  const toastStore = getToastStore()

  let myChannels: Readable<ChannelBasicInfoEx[]> = readable([])
  let filterValue: string = ''

  const modalStore = getModalStore()
  function onCreateChannelHandler() {
    modalStore.trigger({
      type: 'component',
      component: { ref: ChannelCreateModel }
    })
  }

  function gotoChannel(channelId: string) {
    if (channelId != currentChannel) {
      goto(`/_/messages/${channelId}`)
    }
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      myChannels = await myState.loadMyChannelsStream()
      await myState.refreshMyChannels(signal)
      const timer = setInterval(() => {
        if (signal.aborted) {
          clearInterval(timer)
          return
        }

        isActive() && myState.refreshMyChannels(signal)
      }, 10000)
    }, toastStore)
    return abort
  })

  $: currentChannel = $page.params['channel'] || ''
  $: channels =
    $myChannels?.filter((c) => {
      const val = filterValue.trim().toLowerCase()
      return val ? c.name.toLowerCase().includes(val) : true
    }) || []
</script>

<div class="">
  <header
    class="flex h-[60px] flex-row items-center gap-2 border-b border-surface-500/30 p-4 pr-2"
  >
    <input
      class="input h-8 bg-gray/5 leading-8 rounded-container-token"
      type="search"
      bind:value={filterValue}
      placeholder="Filter channels..."
    />
    <button
      type="button"
      class="btn-icon"
      title="Create a channel"
      on:click={onCreateChannelHandler}><span><IconAdd /></span></button
    >
  </header>
  <div class="space-y-4 overflow-y-auto">
    <div class="px-4 py-2 text-sm opacity-50"><span>Channels</span></div>
    <div class="!mt-0 flex flex-col space-y-1">
      {#each channels as channel}
        <button
          type="button"
          class="flex w-full items-center gap-2 px-4 py-2 {channel.channelId ===
          currentChannel
            ? 'variant-soft-primary'
            : 'bg-surface-hover-token'}"
          on:click={() => gotoChannel(channel.channelId)}
        >
          <div class="relative inline-block">
            {#if channel.my_setting.unread > 0}
              <span
                class="badge-icon absolute -right-0 -top-0 z-10 size-2 bg-red-500"
              ></span>
            {/if}
            <Avatar initials={channel.name} fill="fill-white" width="w-10" />
          </div>
          <div class="flex-1">
            <div class="flex flex-row items-center justify-between text-sm">
              <span>
                {channel.name}
              </span>
              {#if channel.my_setting.mute}
                <span class="text-gray/40 *:size-3"
                  ><IconNotificationOffLine /></span
                >
              {:else if channel.my_setting.unread > 0}
                <span class="text-xs">{channel.my_setting.unread}</span>
              {/if}
            </div>
            <div class="flex flex-row text-xs">
              <span class="text-gray/60">
                {channel.latest_message_user.name}
              </span>
              <span class="pl-1 text-gray/40">
                {channel.latest_message_time}
              </span>
            </div>
          </div>
        </button>
      {/each}
    </div>
  </div>
</div>
