<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type ProfileInfo,
    type UpdateProfileInput
  } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { authStore } from '$lib/stores/auth'
  import { toastRun } from '$lib/stores/toast'
  import { md } from '$lib/utils/markdown'
  import { toDisplayUserInfo, type MyMessageState } from '$lib/stores/message'
  import { unwrapOption } from '$lib/types/result'
  import { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { derived, type Readable } from 'svelte/store'
  import ChannelCreateModel from './ChannelCreateModel.svelte'
  import UserRegisterModel from './UserRegisterModel.svelte'
  import PasswordModel from './PasswordModel.svelte'

  export let userId: Principal | string
  export let myState: MyMessageState

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let userInfo: Readable<UserInfo & ProfileInfo>
  let myInfo: Readable<(UserInfo & ProfileInfo) | null>

  $: isMe = myState.id === $userInfo?.id.toText()
  $: isReady = myState.isReady2()
  $: isFowllowing =
    ($userInfo &&
      $myInfo?.following
        .at(0)
        ?.some((id) => id.compareTo($userInfo.id) == 'eq')) ||
    false

  let followingSubmitting = ''
  function onFollowHandler(user: Principal, fowllowing: boolean = true) {
    toastRun(async () => {
      if (myState.principal.isAnonymous()) {
        await authStore.signIn({})
        window.location.reload()
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModel,
            props: {
              myState,
              onFinished: async () => {
                myInfo = await myState.agent.subscribeProfile()
              }
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
      }
    }, toastStore).finally(() => {
      followingSubmitting = ''
    })
  }

  async function onCreateChannelHandler(id: Principal) {
    if (myState.principal.isAnonymous()) {
      await authStore.signIn({})
      window.location.reload()
    } else if (!$myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModel,
          props: {
            myState,
            onFinished: async () => {
              myInfo = await myState.agent.subscribeProfile()
            }
          }
        }
      })
    } else {
      // try to fetch the latest ecdh public key
      const user = await myState.tryFetchProfile(id)
      if (!user) {
        return
      }

      if (!isReady) {
        const iv = await myState.myIV()
        const mk = await myState.masterKey(iv)
        isReady = !!mk && mk.isOpened() && myState.masterKeyKind() === mk.kind
        if (!isReady) {
          modalStore.close()
          modalStore.trigger({
            type: 'component',
            component: {
              ref: PasswordModel,
              props: {
                myState: myState,
                masterKey: mk,
                onFinished: () => {
                  isReady = myState.isReady2()
                }
              }
            }
          })
        }
        return
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

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(async function () {
      myInfo = await myState.agent.subscribeProfile()
      console.log('myInfo', userId, $myInfo)
      if (
        (typeof userId === 'string' && userId === myState.id) ||
        (userId instanceof Principal &&
          userId.compareTo(myState.principal) == 'eq')
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
        setTimeout(() => goto('/'), 1000)
      }
    })
    return abort
  })
</script>

{#if $userInfo}
  {@const display = toDisplayUserInfo($userInfo)}
  <div class="mx-auto flex w-full flex-col items-center gap-1">
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
        <span class="text-neutral-500">@{display.username}</span>
      {/if}
    </p>
    {#if $userInfo.bio}
      <div class="content-markdown">
        {@html md.render($userInfo.bio)}
      </div>
    {/if}
    <p class="mt-2 flex flex-row items-center gap-1 text-sm text-neutral-500">
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
          class="variant-filled-primary btn btn-sm w-32"
          on:click={() => onCreateChannelHandler($userInfo.id)}
        >
          <span>Message</span>
        </button>
      </div>
    {/if}
  </div>
{:else}
  <div class="mt-16 grid justify-center">
    <span class="text-panda *:size-10"><Loading /></span>
  </div>
{/if}
