<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import { Avatar, getModalStore } from '@skeletonlabs/skeleton'
  import { type Readable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'
  import ChannelDetail from './ChannelDetail.svelte'
  import MyChannelList from './MyChannelList.svelte'
  import ProfileDetail from './ProfileDetail.svelte'

  export let myInfo: Readable<UserInfo>

  const modalStore = getModalStore()
  function onMeHandler() {
    goto('/_/messages')
  }

  function onCreateChannelHandler() {
    modalStore.trigger({
      type: 'component',
      component: { ref: ChannelCreateModel }
    })
  }

  $: channelId = $page.params['channel'] || ''
</script>

<section
  class="card mt-4 h-full w-full max-w-4xl rounded-b-none rounded-t-2xl bg-white"
>
  <div class="chat grid h-full w-full grid-cols-1 lg:grid-cols-[30%_1fr]">
    <!-- Navigation -->
    <div
      class="hidden grid-rows-[auto_1fr_auto] border-r border-surface-500/30 lg:grid"
    >
      <!-- Header -->
      <header
        class="flex flex-row items-center gap-2 border-b border-surface-500/30 p-2"
      >
        <input
          class="input bg-gray/5 rounded-container-token"
          type="search"
          placeholder="Filter channels..."
        />
        <button
          type="button"
          class="btn-icon"
          title="Create a channel"
          on:click={onCreateChannelHandler}><span><IconAdd /></span></button
        >
      </header>
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
              >({$myInfo.username[0] || 'get username'})</span
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
      <ProfileDetail userId={$myInfo.id} />
    {/if}
  </div>
</section>
