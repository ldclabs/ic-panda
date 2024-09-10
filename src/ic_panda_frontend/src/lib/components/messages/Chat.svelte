<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import { MyMessageState } from '$src/lib/stores/message'
  import { Avatar } from '@skeletonlabs/skeleton'
  import { type Readable } from 'svelte/store'

  import ChannelDetail from './ChannelDetail.svelte'
  import MyChannelList from './MyChannelList.svelte'
  import ProfileDetail from './ProfileDetail.svelte'

  export let myState: MyMessageState // force reactivity when 'myState' updated
  export let myInfo: Readable<UserInfo>

  function onMeHandler() {
    goto('/_/messages')
  }

  $: channelId = $page.params['channel'] || ''
</script>

<section
  class="card mt-4 h-full w-full max-w-4xl rounded-b-none rounded-t-2xl bg-white"
>
  <div
    class="grid h-full w-full grid-cols-1 sm:grid-cols-[220px_1fr] md:grid-cols-[280px_1fr]"
  >
    <!-- Navigation -->
    <div class="grid grid-rows-[1fr_auto] border-surface-500/30 sm:border-r">
      <!-- List -->
      <MyChannelList />
      <!-- Footer -->
      <footer class="border-t border-surface-500/30">
        <button
          type="button"
          class="flex w-full items-center space-x-2 p-2"
          on:click={onMeHandler}
        >
          <Avatar
            initials={$myInfo.name}
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
    {#if channelId}
      {#key channelId}
        <ChannelDetail {channelId} />
      {/key}
    {:else}
      <div
        class="grid-row-[1fr] grid max-h-[calc(100dvh-76px)] items-start gap-6 bg-white sm:rounded-tr-2xl"
      >
        <ProfileDetail userId={myState.id} />
      </div>
    {/if}
  </div>
</section>
