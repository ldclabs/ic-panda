<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type ProfileInfo,
    type UpdateProfileInput
  } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import { ErrorLogs, toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import { isNotificationSupported } from '$lib/utils/window'
  import {
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$lib/stores/message'
  import { MessageAgent } from '$lib/stores/message_agent'
  import { errMessage, unwrapOption } from '$lib/types/result'
  import { Principal } from '@dfinity/principal'
  import {
    Avatar,
    getModalStore,
    getToastStore,
    SlideToggle
  } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { derived, writable, type Readable, type Writable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'
  import ProfileEditModel from './ProfileEditModel.svelte'
  import UserRegisterModel from './UserRegisterModel.svelte'

  export let userId: Principal | string
  export let myState: MyMessageState

  // local: gnhwq-7p3rq-chahe-22f7s-btty6-ntken-g6dff-xwbyd-4qfse-37euh-5ae
  const PandaID = Principal.fromText(
    'nmob2-y6p4k-rp5j7-7x2mo-aqceq-lpie2-fjgw7-nkjdu-bkoe4-zjetd-wae'
  )

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myFollowing: Writable<DisplayUserInfo[]> = writable([])

  let userInfo: Readable<UserInfo & ProfileInfo>
  let myInfo: Readable<(UserInfo & ProfileInfo) | null>

  let grantedNotification =
    isNotificationSupported && Notification.permission === 'granted'
  let displayDebug = false

  $: pathname = $page?.url?.pathname || ''
  $: myID = $myInfo?.id
  $: isMe = (myID && $userInfo?.id.compareTo(myID)) == 'eq'
  $: isFowllowing =
    ($userInfo &&
      $myInfo?.following
        .at(0)
        ?.some((id) => id.compareTo($userInfo.id) == 'eq')) ||
    false

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
      const user = await myState.api.update_my_name(profile.name)
      await myState.agent.setUser(user)
    }

    const bio = profile.bio.trim()
    if (bio !== $userInfo.bio) {
      const profile = await myState.agent.profileAPI.update_profile({
        bio: [bio],
        remove_channels: [],
        upsert_channels: [],
        follow: [],
        unfollow: []
      })
      await myState.agent.setProfile(profile)
    }
  }

  function onMeHandler() {
    if ($userInfo.username.length == 1) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: ProfileEditModel,
          props: {
            myState,
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
      if (myState.principal.isAnonymous()) {
        await authStore.signIn({})
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
        const input: UpdateProfileInput = fowllowing
          ? {
              bio: [],
              remove_channels: [],
              upsert_channels: [],
              follow: [user],
              unfollow: []
            }
          : {
              bio: [],
              remove_channels: [],
              upsert_channels: [],
              follow: [],
              unfollow: [user]
            }
        const profile = await myState.agent.profileAPI.update_profile(input)
        await myState.agent.setProfile(profile)
        await loadMyFollowing()
      }
    }, toastStore).finally(() => {
      followingSubmitting = ''
    })
  }

  async function onCreateChannelHandler(id: Principal) {
    if (!myState || myState.principal.isAnonymous()) {
      await authStore.signIn({})
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
      if (!myState.isReady()) {
        const iv = await myState.myIV()
        await myState.masterKey(iv)
      }

      modalStore.trigger({
        type: 'component',
        component: {
          ref: ChannelCreateModel,
          props: {
            myState,
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
        await myState.agent.setLocal<string>(MessageAgent.KEY_NOTIFY_PERM, perm)
      } else {
        await myState.agent.setLocal<string>(
          MessageAgent.KEY_NOTIFY_PERM,
          'granted'
        )
      }
    } else {
      await myState.agent.setLocal<string>(
        MessageAgent.KEY_NOTIFY_PERM,
        'denied'
      )
    }
  }

  let clearCachedMessagesSubmitting = false
  function onClearCachedMessages() {
    clearCachedMessagesSubmitting = true
    myState.clearCachedMessages().finally(async () => {
      await sleep(1000)
      clearCachedMessagesSubmitting = false
    })
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(async function () {
      myInfo = await myState.agent.subscribeProfile()
      if (
        (typeof userId === 'string' && userId === myState.id) ||
        (userId instanceof Principal && myID && userId.compareTo(myID) == 'eq')
      ) {
        userInfo = derived(myInfo, ($info, set) => {
          if ($info) {
            set({
              ...$info
            })
          }
        })
      } else {
        userInfo = await myState.loadProfile(userId)
      }
    }, toastStore)

    onfinally(async () => {
      await tick()

      if (!$userInfo) {
        const fallbackUrl = $myInfo ? '/_/messages' : '/'
        setTimeout(() => goto(fallbackUrl), 1000)
      } else {
        if (myID && $userInfo.id.compareTo(myID) == 'eq') {
          const perm = await myState.agent.getLocal<string>(
            MessageAgent.KEY_NOTIFY_PERM
          )
          if (perm === 'denied') {
            grantedNotification = false
          }
          await loadMyFollowing()
        }
      }
    })
    return abort
  })

  function errorLogsText(logs: Error[]): string {
    return logs.map((log) => `${errMessage(log)}\n${log.stack}`).join('\n')
  }
</script>

{#if $userInfo}
  {@const display = toDisplayUserInfo($userInfo)}
  <section class="h-[calc(100dvh-60px)] w-full overflow-y-auto md:h-dvh">
    <div
      class="mx-auto flex min-h-full w-full max-w-3xl flex-col items-center gap-1 p-8 pb-20"
    >
      <div class="size-24">
        <Avatar
          initials={display.name}
          src={display.image}
          border="border-4 border-panda/20"
          background="bg-panda"
          fill="fill-white"
          width="size-24"
        />
      </div>
      <p class="relative space-x-1">
        <span class="font-bold">{display.name}</span>
        {#if display.username}
          <span class="text-neutral-600">@{display.username}</span>
        {/if}
        {#if isMe}
          <button
            type="button"
            class="btn absolute right-[-28px] top-1 p-0 text-neutral-600"
            on:click={onMeHandler}
          >
            <span class="*:size-4"><IconEditLine /></span>
          </button>
        {/if}
      </p>
      {#if $userInfo.bio}
        <div class="content-markdown">
          {@html md.render($userInfo.bio)}
        </div>
      {/if}
      <p class="mt-2 flex flex-row items-center gap-1 text-sm text-neutral-600">
        <span>Principal: {display._id}</span>
        <TextClipboardButton textValue={display._id} />
      </p>
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
          <div class="mb-2 text-sm opacity-50"
            ><button on:click={() => (displayDebug = !displayDebug)}>
              <span>My Setting</span>
            </button></div
          >
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
        {#if displayDebug}
          <div class="mt-4 flex w-full flex-col">
            <div class="mb-2 text-sm opacity-50"><span>Debug</span></div>
            <div class="flex flex-row items-center gap-4">
              <p>Clear cached messages:</p>
              <button
                type="button"
                class="variant-ringed btn btn-sm hover:variant-ghost-warning"
                disabled={clearCachedMessagesSubmitting}
                on:click={onClearCachedMessages}
              >
                <span>Clear (safe)</span>
                <span
                  class="text-panda *:size-4 {clearCachedMessagesSubmitting
                    ? ''
                    : 'hidden'}"
                >
                  <IconCircleSpin />
                </span>
              </button>
            </div>
            {#if ErrorLogs.length > 0}
              {@const value = errorLogsText(ErrorLogs)}
              <div class="flex flex-row items-center gap-4">
                <p>Error logs:</p>
                <p class="text-warning-500">{ErrorLogs.length}</p>
                <TextClipboardButton textValue={value} />
              </div>
            {/if}
          </div>
        {/if}
      {/if}
      {#if isMe && $myFollowing.length > 0}
        <div class="mt-4 flex w-full flex-col gap-4">
          <div class="">
            <span class="text-sm opacity-50">Following</span>
          </div>
          <div class="!mt-0 space-y-2">
            {#each $myFollowing as member (member._id)}
              <div class="grid items-center gap-1 sm:grid-cols-[1fr_auto]">
                <div class="flex flex-row items-center space-x-2">
                  <Avatar
                    initials={member.name}
                    src={member.image}
                    fill="fill-white"
                    width="w-10"
                  />
                  <span class="ml-1">{member.name}</span>
                  {#if member.username}
                    <a
                      class="text-neutral-600 underline underline-offset-4"
                      href="{APP_ORIGIN}/{member.username}"
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
      {#if pathname.startsWith('/_/profile') && userId
          .toString()
          .toUpperCase() !== 'PANDA'}
        <div
          class="mx-auto mt-48 flex flex-col items-center space-y-2 self-end text-sm"
        >
          <div class="flex flex-row items-center">
            <a
              class="flex flex-row items-center space-x-1"
              href="https://dmsg.net/PANDA"
            >
              <Avatar src="/_assets/logo.svg" fill="fill-white" width="w-8" />
              <span class="ml-1 truncate">ICPanda</span>
              <span class="text-neutral-600">@PANDA</span>
            </a>
            <button
              type="button"
              title="End-to-end encrypted message"
              class="variant-ringed-primary btn btn-sm ml-2 w-32 hover:variant-soft-primary"
              on:click={() => onCreateChannelHandler(PandaID)}
            >
              <span>Message</span>
            </button>
          </div>
          <p class="text-neutral-600"
            >If you encounter any issues, please message me.</p
          >
        </div>
      {/if}
    </div>
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
