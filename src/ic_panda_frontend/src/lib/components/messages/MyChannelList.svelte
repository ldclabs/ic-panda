<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconInfo from '$lib/components/icons/IconInfo.svelte'
  import IconNotificationOffLine from '$lib/components/icons/IconNotificationOffLine.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { initFocus, isActive } from '$lib/utils/window'
  import {
    type ChannelBasicInfoEx,
    type MyMessageState
  } from '$src/lib/stores/message'
  import {
    Avatar,
    getModalStore,
    getToastStore,
    popup
  } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'

  export let myState: MyMessageState

  const toastStore = getToastStore()

  let myChannels: Readable<ChannelBasicInfoEx[]>
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
    const { abort } = toastRun(
      async (signal: AbortSignal, abortingQue: (() => void)[]) => {
        myChannels = await myState.loadMyChannelsStream()

        const debouncedrefreshMyChannels = debounce(
          async () => {
            await myState.refreshMyChannels(signal)
          },
          3000,
          { immediate: true }
        )
        abortingQue.push(debouncedrefreshMyChannels.clear)
        debouncedrefreshMyChannels()

        const focusAbort = initFocus(() => {
          debouncedrefreshMyChannels()
        })
        abortingQue.push(focusAbort)

        const timer = setInterval(() => {
          isActive() && debouncedrefreshMyChannels()
        }, 10000)

        abortingQue.push(() => clearInterval(timer))
      },
      toastStore
    )

    return abort
  })

  $: currentChannel = ($page?.params || {})['channel'] || ''
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
  <div class="overflow-y-auto">
    <div class="flex flex-row items-center gap-2 p-2">
      <span class="text-sm opacity-50">Channels</span>
      <button
        class=""
        use:popup={{
          event: 'click',
          target: 'ChannelTipHover',
          middleware: {
            size: { availableWidth: 300, availableHeight: 40 }
          }
        }}
      >
        <span class="opacity-50 *:size-5">
          <IconInfo />
        </span>
      </button>
      <div
        class="card z-10 max-w-80 bg-surface-900 p-2 py-1"
        data-popup="ChannelTipHover"
      >
        <p class="min-w-0 text-balance break-words text-white">
          <span
            >In addition to encrypted chats, you can also store confidential
            information.<br />It's encrypted on-chain and synced across devices,
            with only you able to read and decrypt it.</span
          >
        </p>
        <div class="arrow bg-surface-900" />
      </div>
    </div>
    {#if channels.length === 0}
      <div class="px-4 py-2 text-sm"
        ><span
          >In addition to encrypted chats, you can also store confidential
          information.<br />It's encrypted on-chain and synced across devices,
          with only you able to read and decrypt it.</span
        ></div
      >
    {/if}
    <div class="flex flex-col divide-y divide-gray/5">
      {#each channels as channel}
        <button
          type="button"
          class="flex w-full items-center gap-2 p-2 {channel.channelId ===
          currentChannel
            ? 'variant-soft-primary'
            : 'bg-surface-hover-token'}"
          on:click={() => gotoChannel(channel.channelId)}
        >
          <div class="relative inline-block">
            {#if channel.my_setting.unread > 0 || channel.my_setting.ecdh_remote.length > 0}
              <span
                class="badge-icon absolute -right-0 -top-0 z-10 size-2 bg-red-500"
              ></span>
            {/if}
            <Avatar
              initials={channel.name}
              src={channel.image}
              fill="fill-white"
              width="w-10"
            />
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
