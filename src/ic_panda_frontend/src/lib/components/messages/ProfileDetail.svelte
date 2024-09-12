<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { signIn } from '$lib/services/auth'
  import { toastRun } from '$lib/stores/toast'
  import { md } from '$lib/utils/markdown'
  import {
    myMessageStateAsync,
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { unwrapOption } from '$src/lib/types/result'
  import { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import {
    readable,
    writable,
    type Readable,
    type Writable
  } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'
  import ProfileEditModel from './ProfileEditModel.svelte'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let userId: Principal | string

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myFollowing: Writable<DisplayUserInfo[]> = writable([])

  let myID: Principal
  let myState: MyMessageState
  let userInfo: Readable<UserInfo & ProfileInfo>
  let myInfo: (UserInfo & ProfileInfo) | null = null

  async function loadMyFollowing() {
    const res: DisplayUserInfo[] = []
    if (myInfo) {
      const users = await myState.batchLoadUsersInfo(
        myInfo.following.at(0) || []
      )
      res.push(...users.map(toDisplayUserInfo))
    }
    myFollowing.set(res)
  }

  async function loadProfile(user: Principal | string) {
    myState = await myMessageStateAsync()
    myID = myState.principal

    if (
      (typeof user === 'string' && user === myState.id) ||
      (user instanceof Principal && user.compareTo(myID) == 'eq')
    ) {
      myInfo = await myState.loadMyProfile()
      userInfo = readable(myInfo)
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
      myInfo = await myState.updateProfile({
        bio: [bio],
        remove_channels: [],
        upsert_channels: [],
        follow: [],
        unfollow: []
      })
    }
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

  let followingSubmitting = ''
  function onFollowHandler(user: Principal, fowllowing: boolean = true) {
    toastRun(async () => {
      if (myState?.principal.isAnonymous()) {
        const res = await signIn({})
        myState = await myMessageStateAsync()
        myInfo = await myState.loadMyProfile().catch(() => null)
        if (!myInfo && res.success == 'ok') {
          modalStore.trigger({
            type: 'component',
            component: {
              ref: UserRegisterModel,
              props: {}
            }
          })
        }
      } else if (!myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {}
          }
        })
      } else if (!followingSubmitting) {
        followingSubmitting = user.toText()
        if (fowllowing) {
          myInfo = await myState.updateProfile({
            bio: [],
            remove_channels: [],
            upsert_channels: [],
            follow: [user],
            unfollow: []
          })
        } else {
          myInfo = await myState.updateProfile({
            bio: [],
            remove_channels: [],
            upsert_channels: [],
            follow: [],
            unfollow: [user]
          })
        }
        await loadMyFollowing()
      }
    }, toastStore).finally(() => {
      followingSubmitting = ''
    })
  }

  async function onCreateChannelHandler(id: Principal) {
    if (myState?.principal.isAnonymous()) {
      const res = await signIn({})
      myState = await myMessageStateAsync()
      myInfo = await myState.loadMyProfile().catch(() => null)
      if (!myInfo && res.success == 'ok') {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {}
          }
        })
      }
    } else if (!myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModel,
          props: {}
        }
      })
    } else {
      // try to fetch the latest ecdh public key
      const user = await myState.tryFetchProfile(id)
      if (!user) {
        return
      }

      modalStore.trigger({
        type: 'component',
        component: {
          ref: ChannelCreateModel,
          props: {
            channelName: `${user.name}, ${myInfo!.name}`,
            add_managers: [
              [user.id, unwrapOption(user.ecdh_pub as [] | [Uint8Array])]
            ]
          }
        }
      })
    }
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(
      () => loadProfile(userId),
      toastStore
    )
    onfinally(async (info) => {
      if (!info) {
        setTimeout(() => goto('/_/messages'), 2000)
      } else {
        if (info.id.compareTo(myID) == 'eq') {
          if (
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
          await loadMyFollowing()
        } else {
          myInfo = await myState.loadMyProfile().catch(() => null)
        }
      }
    })
    return abort
  })

  $: isMe = $userInfo?.id.compareTo(myID) == 'eq'
  $: isFowllowing =
    myInfo?.following.at(0)?.some((id) => id.compareTo($userInfo.id) == 'eq') ||
    false
</script>

{#if $userInfo}
  <section
    class="flex w-full flex-col items-center gap-0 overflow-y-auto p-4 pb-40 sm:mt-16"
  >
    <Avatar
      initials={$userInfo.name}
      border="border-4 border-white"
      background="bg-panda"
      fill="fill-white"
      width="w-32"
    />
    <p class="relative">
      <span>{$userInfo.name}</span>
      {#if isMe}
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
      <div class="content-markdown mt-2">
        {@html md.render($userInfo.bio)}
      </div>
    {/if}
    {#if !isMe}
      <div class="mt-4 flex flex-row gap-4">
        <button
          type="button"
          class="{isFowllowing
            ? 'variant-ghost-primary'
            : 'variant-filled'} group btn btn-sm w-32"
          disabled={followingSubmitting !== ''}
          on:click={() => onFollowHandler($userInfo.id, !isFowllowing)}
        >
          {#if isFowllowing}
            <span class="group-hover:hidden">Following</span>
            <span class="hidden text-error-500 group-hover:inline"
              >Unfollow</span
            >
          {:else}
            <span class="">Follow</span>
          {/if}
          <span
            class="text-panda *:size-4 {followingSubmitting ? '' : 'hidden'}"
          >
            <IconCircleSpin />
          </span>
        </button>
        <button
          type="button"
          title="End-to-end encrypted message"
          class="variant-filled btn btn-sm w-32"
          on:click={() => onCreateChannelHandler($userInfo.id)}
        >
          <span>Message</span>
        </button>
      </div>
    {/if}
    {#if isMe && $myFollowing.length > 0}
      <div class="mt-4 flex w-full max-w-2xl flex-col gap-4">
        <div class="">
          <span class="text-sm opacity-50">Following</span>
        </div>
        <div class="!mt-0 space-y-2">
          {#each $myFollowing as member (member._id)}
            <div class="grid items-center gap-1 sm:grid-cols-[1fr_auto]">
              <div class="flex flex-row items-center space-x-2">
                <Avatar initials={member.name} fill="fill-white" width="w-10" />
                <span class="ml-1">{member.name}</span>
                {#if member.username}
                  <a
                    class="text-gray/60 underline underline-offset-4"
                    href="https://panda.fans/{member.username}"
                    >@{member.username}</a
                  >
                {/if}
              </div>
              <div class="flex flex-row items-center justify-end space-x-2">
                <button
                  class="variant-ghost-warning btn btn-sm"
                  type="button"
                  disabled={followingSubmitting !== '' || !member.src}
                  on:click={() =>
                    member.src && onFollowHandler(member.src.id, false)}
                >
                  <span>Unfollow</span>
                  <span
                    class="text-panda *:size-4 {followingSubmitting ===
                    member._id
                      ? ''
                      : 'hidden'}"
                  >
                    <IconCircleSpin />
                  </span>
                </button>
                <button
                  class="variant-filled btn btn-sm"
                  type="button"
                  on:click={() =>
                    member.src && onCreateChannelHandler(member.src.id)}
                >
                  <span>Message</span>
                </button>
              </div>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </section>
{:else}
  <div class="mt-16 self-center">
    <span class="text-panda *:size-10"><Loading /></span>
  </div>
{/if}

<style>
  .content-markdown :global(a) {
    text-underline-offset: 4px;
    text-decoration-line: underline;
    font-weight: 500;
  }
</style>
