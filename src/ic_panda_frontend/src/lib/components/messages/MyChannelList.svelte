<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import {
    ChannelAPI,
    type ChannelBasicInfo
  } from '$lib/canisters/messagechannel'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { Avatar } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { readable, type Readable } from 'svelte/store'

  let myState: MyMessageState
  let myChannels: Readable<ChannelBasicInfo[]> = readable([])
  let currentChannel: string = ''

  function gotoChannel(cid: string) {
    if (cid != currentChannel) {
      currentChannel = cid
      goto(`/_/messages/${cid}`)
    }
  }

  async function activeChannel(cid: string) {
    const { canister } = ChannelAPI.parseChannelParam(cid)
    if (canister) {
      currentChannel = cid
      return
    }

    goto('/_/messages')
  }

  onMount(async () => {
    myState = await myMessageStateAsync()
    myChannels = await myState.loadMyChannelsStream()
    activeChannel($page.params['channel'] || '')
  })
</script>

<div class="space-y-4 overflow-y-auto">
  <div class="p-2 text-sm opacity-50"><span>Channels</span></div>
  <div class="!mt-0 flex flex-col space-y-1">
    {#each $myChannels as channel}
      {@const channelId = ChannelAPI.channelParam(channel)}
      <button
        type="button"
        class="flex w-full items-center space-x-4 p-2 {channelId ===
        currentChannel
          ? 'variant-soft-primary'
          : 'bg-surface-hover-token'}"
        on:click={() => gotoChannel(channelId)}
      >
        <Avatar initials={channel.name} fill="fill-white" width="w-8" />
        <span class="flex-1 text-start">
          {channel.name}
        </span>
      </button>
    {/each}
  </div>
</div>
