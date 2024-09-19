<script lang="ts">
  import { goto } from '$app/navigation'
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import { type ProfileInfo } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { signIn } from '$lib/services/auth'
  import { ErrorLogs, toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import { KEY_NOTIFY_PERM, KVS } from '$src/lib/stores/kvstore'
  import {
    myMessageStateAsync,
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { errMessage, unwrapOption } from '$src/lib/types/result'
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

  // local: gnhwq-7p3rq-chahe-22f7s-btty6-ntken-g6dff-xwbyd-4qfse-37euh-5ae
  const PandaID = Principal.fromText(
    'nmob2-y6p4k-rp5j7-7x2mo-aqceq-lpie2-fjgw7-nkjdu-bkoe4-zjetd-wae'
  )

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myFollowing: Writable<DisplayUserInfo[]> = writable([])
  const myInfo: Writable<(UserInfo & ProfileInfo) | null> = writable(null)

  let myState: MyMessageState
  let userInfo: Readable<UserInfo & ProfileInfo>

  let grantedNotification = Notification.permission === 'granted'
  let displayDebug = false

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
      if (!myState.isReady()) {
        const iv = await myState.myIV()
        await myState.masterKey(iv)
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

  function errorLogsText(logs: Error[]): string {
    return logs.map((log) => `${errMessage(log)}\n${log.stack}`).join('\n')
  }

  $: myID = $myInfo?.id
  $: isMe = (myID && $userInfo?.id.compareTo(myID)) == 'eq'
  $: isFowllowing =
    $myInfo?.following
      .at(0)
      ?.some((id) => id.compareTo($userInfo.id) == 'eq') || false
</script>

{#if $userInfo}
  {@const display = toDisplayUserInfo($userInfo)}
  <section
    class="flex w-full flex-col items-center gap-0 overflow-y-auto p-8 pb-40 sm:mt-16"
  >
    <Avatar
      initials={display.name}
      src={display.image}
      border="border-4 border-panda/20"
      background="bg-panda"
      fill="fill-white"
      width="w-32"
    />
    <p class="relative space-x-1">
      <span>{display.name}</span>
      {#if display.username}
        <span class="text-gray/60">@{display.username}</span>
      {/if}
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
    <p class="flex flex-row items-center gap-1 text-sm text-gray/60">
      <span>{display._id}</span>
      <TextClipboardButton textValue={display._id} />
    </p>
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
                    class="text-gray/60 underline underline-offset-4"
                    href="{APP_ORIGIN}/{member.username}">@{member.username}</a
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
  {#if userId.toString().toUpperCase() !== 'PANDA'}
    <div class="m-auto flex flex-col items-center space-y-2 self-end text-sm">
      <div class="flex flex-row items-center">
        <a
          class="flex flex-row items-center space-x-1"
          href="https://panda.fans/PANDA"
        >
          <Avatar src="/_assets/logo.svg" fill="fill-white" width="w-8" />
          <span class="ml-1 truncate">ICPanda DAO</span>
          <span class="text-gray/60">@PANDA</span>
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
      <p class="text-gray/60">If you encounter any issues, please message me.</p
      >
    </div>
  {/if}
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
