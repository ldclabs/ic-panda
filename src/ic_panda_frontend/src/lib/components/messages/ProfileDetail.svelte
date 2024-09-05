<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { md } from '$lib/utils/markdown'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import type { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let userId: Principal

  const modalStore = getModalStore()
  let myID: Principal
  let myState: MyMessageState
  let myInfo: Readable<UserInfo>
  let userInfo: Readable<UserInfo & ProfileInfo>

  async function loadProfile(user: Principal | string) {
    myState = await myMessageStateAsync()
    myInfo = myState.info
    myID = myState.principal
    userInfo = await myState.loadProfile(user)
  }

  function onMeHandler() {
    modalStore.trigger({
      type: 'component',
      component: { ref: UserRegisterModel }
    })
  }

  onMount(() => {
    loadProfile(userId).catch((err) => console.error('loadProfile ERROR:', err))
  })
</script>

<div
  class="grid-row-[1fr] grid max-h-[calc(100dvh-76px)] items-start gap-6 rounded-tr-2xl bg-white"
>
  {#if $userInfo}
    <section
      class="mt-16 flex w-full flex-col items-center gap-0 overflow-y-auto p-4 pb-40"
    >
      <Avatar
        initials={$userInfo.name}
        background="bg-panda"
        fill="fill-white "
        width="w-24"
      />
      <p class="relative">
        <span>{$userInfo.name}</span>
        {#if userId.compareTo(myID) == 'eq'}
          <button
            type="button"
            class="btn absolute right-[-40px] p-0 text-gray/60"
            on:click={onMeHandler}
          >
            <span>Edit</span>
          </button>
        {/if}
      </p>
      {#if $userInfo.username.length === 1}
        <p class="text-gray/60">@{$userInfo.username[0]}</p>
      {/if}
      {#if $userInfo.bio}
        <div>
          {@html md.render($userInfo.bio)}
        </div>
      {/if}
    </section>
  {:else}
    <div class="self-center">
      <span class="text-panda *:size-10"><Loading /></span>
    </div>
  {/if}
</div>
