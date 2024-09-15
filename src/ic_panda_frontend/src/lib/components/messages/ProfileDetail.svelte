<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { signIn } from '$lib/services/auth'
  import { toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import { KEY_NOTIFY_PERM, KVS } from '$src/lib/stores/kvstore'
  import {
    myMessageStateAsync,
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { unwrapOption } from '$src/lib/types/result'
  import { Principal } from '@dfinity/principal'
  import {
    Avatar,
    getModalStore,
    getToastStore,
    SlideToggle
  } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { derived, writable, type Readable, type Writable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'
  import ProfileEditModel from './ProfileEditModel.svelte'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let userId: Principal | string

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myFollowing: Writable<DisplayUserInfo[]> = writable([])
  const myInfo: Writable<(UserInfo & ProfileInfo) | null> = writable(null)

  let myState: MyMessageState
  let userInfo: Readable<UserInfo & ProfileInfo>

  let grantedNotification = Notification.permission === 'granted'

  async function loadMyFollowing() {
    const res: DisplayUserInfo[] = []
    if ($myInfo) {
      const users = await myState.batchLoadUsersInfo(
        $myInfo.following.at(0) || []
      )
      res.push(...users.map(toDisplayUserInfo))
    }
    myFollowing.set(res)
  }

  async function saveProfile(profile: UserInfo & ProfileInfo) {
    if (profile.name && profile.name !== $userInfo.name) {
      await myState.api.update_my_name(profile.name)
      await myState.api.refreshMyInfo()
    }

    const bio = profile.bio.trim()
    if (bio !== $userInfo.bio) {
      await myState.updateProfile({
        bio: [bio],
        remove_channels: [],
        upsert_channels: [],
        follow: [],
        unfollow: []
      })
    }

    await myState.refreshAllState(false)
    myInfo.set(await myState.loadMyProfile())
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
          props: {
            myState
          }
        }
      })
    }
  }

  let followingSubmitting = ''
  function onFollowHandler(user: Principal, fowllowing: boolean = true) {
    toastRun(async () => {
      if (!myState || myState.principal.isAnonymous()) {
        await signIn({})
        myState = await myMessageStateAsync()
        const rt = await myState.loadMyProfile().catch(() => null)
        myInfo.set(rt)
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {
              myState
            }
          }
        })
      } else if (!followingSubmitting) {
        followingSubmitting = user.toText()
        if (fowllowing) {
          myInfo.set(
            await myState.updateProfile({
              bio: [],
              remove_channels: [],
              upsert_channels: [],
              follow: [user],
              unfollow: []
            })
          )
        } else {
          myInfo.set(
            await myState.updateProfile({
              bio: [],
              remove_channels: [],
              upsert_channels: [],
              follow: [],
              unfollow: [user]
            })
          )
        }
        await loadMyFollowing()
      }
    }, toastStore).finally(() => {
      followingSubmitting = ''
    })
  }

  async function onCreateChannelHandler(id: Principal) {
    if (!myState || myState.principal.isAnonymous()) {
      await signIn({})
      myState = await myMessageStateAsync()
      const rt = await myState.loadMyProfile().catch(() => null)
      myInfo.set(rt)
    } else if (!$myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModel,
          props: {
            myState
          }
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
            channelName: `${user.name}, ${$myInfo!.name}`,
            add_managers: [
              [user.id, unwrapOption(user.ecdh_pub as [] | [Uint8Array])]
            ]
          }
        }
      })
    }
  }

  async function onRequestNotifications() {
    await sleep(1000)
    if (grantedNotification) {
      if (Notification.permission !== 'granted') {
        const perm = await Notification.requestPermission()
        grantedNotification = perm === 'granted'
        await KVS.set<string>('My', perm, myID!.toText() + KEY_NOTIFY_PERM)
      } else {
        await KVS.set<string>('My', 'granted', myID!.toText() + KEY_NOTIFY_PERM)
      }
    } else {
      await KVS.set<string>('My', 'denied', myID!.toText() + KEY_NOTIFY_PERM)
    }
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(async function () {
      myState = await myMessageStateAsync()

      if (
        (typeof userId === 'string' && userId === myState.id) ||
        (userId instanceof Principal && myID && userId.compareTo(myID) == 'eq')
      ) {
        const info = await myState.loadMyProfile()
        myInfo.set(info)
        userInfo = derived(myInfo, (info, set) => {
          if (info) {
            set({
              ...info
            })
          }
        })
      } else {
        userInfo = await myState.loadProfile(userId)
      }
      return $userInfo
    }, toastStore)
    onfinally(async (info) => {
      if (!info) {
        setTimeout(() => goto('/_/messages'), 2000)
      } else {
        if (myID && info.id.compareTo(myID) == 'eq') {
          if (
            $page.params['channel'] == 'profile' &&
            info.username.length == 0 &&
            info.created_at < BigInt(Date.now() - 180 * 1000)
          ) {
            modalStore.trigger({
              type: 'component',
              component: {
                ref: UserRegisterModel,
                props: {
                  myState
                }
              }
            })
          }
          const perm = await KVS.get<string>(
            'My',
            myID.toText() + KEY_NOTIFY_PERM
          )
          if (perm === 'denied') {
            grantedNotification = false
          }
          await loadMyFollowing()
        } else {
          myInfo.set(await myState.loadMyProfile().catch(() => null))
        }
      }
    })
    return abort
  })

  $: myID = $myInfo?.id
  $: isMe = (myID && $userInfo?.id.compareTo(myID)) == 'eq'
  $: isFowllowing =
    $myInfo?.following
      .at(0)
      ?.some((id) => id.compareTo($userInfo.id) == 'eq') || false
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
    {:else}
      <div class="mt-4 flex w-full flex-col">
        <div class="mb-2 text-sm opacity-50"><span>My Setting</span></div>
        <div class="flex flex-row items-center gap-4">
          <p>Notifications:</p>
          <SlideToggle
            name="setting-mute"
            active="bg-panda"
            size="sm"
            bind:checked={grantedNotification}
            on:click={onRequestNotifications}
          />
        </div>
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
  <div class="mt-16 grid justify-center">
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
