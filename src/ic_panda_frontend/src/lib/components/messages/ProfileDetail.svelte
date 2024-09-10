<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { md } from '$lib/utils/markdown'
  import {
    myMessageStateAsync,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ProfileEditModel from './ProfileEditModel.svelte'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let userId: Principal | string

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let myID: Principal
  let myState: MyMessageState
  let userInfo: Readable<UserInfo & ProfileInfo>

  async function loadProfile(user: Principal | string) {
    myState = await myMessageStateAsync()
    myID = myState.principal

    if (
      (typeof user === 'string' && user === myState.id) ||
      (user instanceof Principal && user.compareTo(myID) == 'eq')
    ) {
      userInfo = await myState.loadMyProfile()
    } else {
      userInfo = await myState.loadProfile(user)
    }
    return $userInfo
  }

  async function saveProfile(profile: UserInfo & ProfileInfo) {
    if (profile.name !== $userInfo.name) {
      await myState.api.update_my_name(profile.name)
      await myState.api.refreshMyInfo()
    }

    const bio = profile.bio.trim()
    if (bio !== $userInfo.bio) {
      await myState.updateProfile($userInfo.profile_canister, {
        bio: [bio],
        remove_channels: [],
        upsert_channels: [],
        follow: [],
        unfollow: []
      })
    }

    userInfo = await myState.loadMyProfile()
  }

  function onMeHandler() {
    if ($userInfo.username.length == 1) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: ProfileEditModel,
          props: {
            myInfo: userInfo,
            onSave: saveProfile
          }
        }
      })
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModel,
          props: {}
        }
      })
    }
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(
      () => loadProfile(userId),
      toastStore
    )
    onfinally((info) => {
      if (!info) {
        setTimeout(() => goto('/_/messages'), 2000)
      } else if (
        info.id.compareTo(myID) == 'eq' &&
        info.username.length == 0 &&
        info.created_at < BigInt(Date.now() - 180 * 1000)
      ) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {}
          }
        })
      }
    })
    return abort
  })
</script>

{#if $userInfo}
  <section
    class="mt-16 flex w-full flex-col items-center gap-0 overflow-y-auto p-4 pb-40"
  >
    <Avatar
      initials={$userInfo.name}
      border="border-4 border-surface-300-600-token"
      background="bg-panda"
      fill="fill-white"
      width="w-24"
    />
    <p class="relative">
      <span>{$userInfo.name}</span>
      {#if $userInfo.id.compareTo(myID) == 'eq'}
        <button
          type="button"
          class="btn absolute right-[-28px] top-1 p-0 text-gray/60"
          on:click={onMeHandler}
        >
          <span class="*:size-4"><IconEditLine /></span>
        </button>
      {/if}
    </p>
    {#if $userInfo.username.length === 1}
      <p class="text-gray/60">@{$userInfo.username[0]}</p>
    {/if}
    {#if $userInfo.bio}
      <div class="mt-2">
        {@html md.render($userInfo.bio)}
      </div>
    {/if}
  </section>
{:else}
  <div class="mt-16 self-center">
    <span class="text-panda *:size-10"><Loading /></span>
  </div>
{/if}
