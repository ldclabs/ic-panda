<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import { MyMessageState } from '$src/lib/stores/message'
  import { sleep } from '$src/lib/utils/helper'
  import { Avatar } from '@skeletonlabs/skeleton'
  import { setContext } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelDetail from './ChannelDetail.svelte'
  import MyChannelList from './MyChannelList.svelte'
  import ProfileDetail from './ProfileDetail.svelte'

  export let myState: MyMessageState // force reactivity when 'myState' updated
  export let myInfo: Readable<UserInfo>

  let channelsListActive = true

  function onMeHandler() {
    goto('/_/messages/profile')
  }

  async function onChatBack() {
    channelsListActive = true
    await sleep(300)
    goto('/_/messages')
  }

  setContext('onChatBack', onChatBack)

  $: channelId = ($page?.params || {})['channel'] || ''
  $: channelsListActive = channelId === ''
</script>

<section
  class="card h-full w-full rounded-b-none bg-white max-sm:rounded-none sm:mt-4 sm:rounded-t-2xl"
>
  <div
    class="relative h-full w-full sm:grid sm:grid-cols-[220px_1fr] md:grid-cols-[280px_1fr] lg:grid-cols-[300px_1fr]"
  >
    <div
      class="channels-list transition duration-300 ease-out {channelsListActive
        ? ''
        : 'max-sm:-translate-x-full'} grid grid-rows-[1fr_auto] border-r border-surface-500/30 bg-white max-sm:shadow-sm sm:rounded-tl-2xl"
    >
      <MyChannelList {myState} />
      <footer class="border-t border-surface-500/30">
        <button
          type="button"
          class="flex w-full items-center space-x-2 px-4 py-2"
          on:click={onMeHandler}
        >
          <Avatar
            initials={$myInfo.name}
            src={$myInfo.image}
            fill="fill-white"
            background="bg-panda"
            width="w-10"
          />
          <div class="flex-1 text-pretty text-start text-sm"
            ><span>{$myInfo.name}</span><span
              class="ml-1 {$myInfo.username[0]
                ? 'text-gray/50'
                : 'text-gray/40'}"
              >({$myInfo.username[0] || 'Get a username'})</span
            ></div
          >
        </button>
      </footer>
    </div>
    <div
      class="grid-row-[1fr] grid max-h-[calc(100dvh-80px)] sm:rounded-tr-2xl"
    >
      {#key channelId}
        {#if channelId && channelId !== 'profile'}
          <ChannelDetail {channelId} {myState} />
        {:else}
          <div class="h-[60px] px-4 py-2 md:hidden">
            <button class="btn -ml-6" on:click={onChatBack}
              ><IconArrowLeftSLine /></button
            >
          </div>
          <ProfileDetail userId={myState.id} />
        {/if}
      {/key}
    </div>
  </div>
</section>

<style>
  @media (max-width: 640px) {
    .channels-list {
      position: absolute;
      top: 0;
      bottom: 0;
      z-index: 10;
      width: 100%;
    }
  }
</style>
