<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconNotificationOffLine from '$lib/components/icons/IconNotificationOffLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import {
    type ChannelBasicInfoEx,
    type MyMessageState
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { initFocus, isActive } from '$lib/utils/window'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onMount } from 'svelte'
  import { readable, type Readable } from 'svelte/store'
  import ChannelCreateModal from './ChannelCreateModal.svelte'

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  interface Props {
    myState: MyMessageState
  }

  let { myState }: Props = $props()

  let isLoading = $state(true)
  let myChannels: Readable<ChannelBasicInfoEx[]> = $state(readable([]))
  let filterValue: string = $state('')

  const currentChannel = $derived(($page?.params || {})['channel'] || '')
  const channels = $derived.by(() => {
    return ($myChannels || []).filter((c) => {
      const val = filterValue.trim().toLowerCase()
      return val ? c.name.toLowerCase().includes(val) : true
    })
  })

  function onCreateChannelHandler() {
    modalStore.trigger({
      type: 'component',
      component: { ref: ChannelCreateModal, props: { myState } }
    })
  }

  function gotoChannel(channelId: string) {
    if (channelId != currentChannel) {
      goto(`/_/${channelId}`)
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
        isLoading = false
      },
      toastStore
    )

    return abort
  })
</script>

<div class="grid h-[calc(100dvh-60px)] grid-rows-[auto_1fr] md:h-dvh">
  <header class="flex h-[60px] flex-row items-center gap-4 p-4 pr-4">
    <input
      class="input truncate rounded-lg border-0 bg-surface-900/5"
      type="search"
      bind:value={filterValue}
      placeholder="Filter channels..."
    />
    <button
      type="button"
      class="btn btn-icon h-10 rounded-lg bg-surface-900/5 text-surface-500 hover:text-surface-900 dark:bg-surface-700 dark:hover:text-surface-100"
      title="Create a channel"
      onclick={onCreateChannelHandler}
      ><span class="hover:scale-110"><IconAdd /></span></button
    >
  </header>
  <div class="flex flex-col overflow-y-auto pb-10">
    {#each channels as channel}
      <button
        type="button"
        class="flex w-full items-center gap-2 px-4 py-2 {channel.channelId ===
        currentChannel
          ? 'variant-soft-primary'
          : 'bg-surface-hover-token'}"
        onclick={() => gotoChannel(channel.channelId)}
      >
        <div class="relative inline-block">
          {#if channel.my_setting.unread > 0 || channel.my_setting.ecdh_remote.length > 0}
            <span
              class="badge-icon absolute -right-0 -top-0 z-10 size-3 bg-error-500"
            ></span>
          {/if}
          <Avatar
            initials={channel.name}
            src={channel.image}
            fill="fill-white"
            width="w-10"
          />
        </div>
        <div class="min-w-0 flex-1">
          <p class="truncate text-left text-sm">
            {channel.name}
          </p>
          <div
            class="flex flex-row items-center justify-between space-x-1 text-xs text-surface-500"
          >
            <p>
              <span>
                {channel.latest_message_user.name}
              </span>
              <span>
                {channel.latest_message_time}
              </span>
            </p>
            {#if channel.my_setting.mute}
              <span class="text-surface-400 *:size-3"
                ><IconNotificationOffLine /></span
              >
            {:else if channel.my_setting.unread > 0}
              <span class="text-xs">{channel.my_setting.unread}</span>
            {/if}
          </div>
        </div>
      </button>
    {:else}
      <div class="px-4 py-2 text-sm">
        <span
          >In addition to encrypted chats, you can also store confidential
          information.<br />It's encrypted on-chain and synced across devices,
          with only you able to read and decrypt it.</span
        >
      </div>
      {#if isLoading}
        <Loading />
      {/if}
    {/each}
  </div>
</div>
