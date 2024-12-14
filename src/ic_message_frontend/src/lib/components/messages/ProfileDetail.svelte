<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type Link,
    type ProfileInfo,
    type UpdateProfileInput
  } from '$lib/canisters/messageprofile'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconQrCode from '$lib/components/icons/IconQrCode.svelte'
  import LinkItem from '$lib/components/ui/LinkItem.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import QrModal from '$lib/components/ui/QrModal.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import { toDisplayUserInfo, type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { unwrapOption } from '$lib/types/result'
  import { shortId } from '$lib/utils/auth'
  import { md } from '$lib/utils/markdown'
  import { Principal } from '@dfinity/principal'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { derived, type Readable } from 'svelte/store'
  import ChannelCreateModal from './ChannelCreateModal.svelte'
  import PasswordModal from './PasswordModal.svelte'
  import UserRegisterModal from './UserRegisterModal.svelte'

  interface Props {
    /** Exposes parent props to this component. */
    userId: Principal | string
    myState: MyMessageState
  }

  let { userId, myState }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let userInfo: Readable<UserInfo & ProfileInfo>
  let myInfo: Readable<(UserInfo & ProfileInfo) | null>
  let isReady = $state(myState.isReady2())
  let isMe = $state(false)
  let isFowllowing = $state(false)
  let links: Link[] = $state([])

  $effect(() => {
    isMe = myState.id === $userInfo?.id.toText()
    isFowllowing =
      ($userInfo &&
        $myInfo?.following
          .at(0)
          ?.some((id) => id.compareTo($userInfo.id) == 'eq')) ||
      false
    links = $userInfo?.links || []
  })

  let followingSubmitting = $state('')
  function onFollowHandler(user: Principal, fowllowing: boolean = true) {
    toastRun(async () => {
      if (myState.principal.isAnonymous()) {
        await authStore.signIn()
        window.location.reload()
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModal,
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
      await authStore.signIn()
      window.location.reload()
    } else if (!$myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModal,
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
              ref: PasswordModal,
              props: {
                myState: myState,
                masterKey: mk,
                onFinished: () => {
                  isReady = myState.isReady2()
                }
              }
            }
          })
          return
        }
      }

      modalStore.close()
      modalStore.trigger({
        type: 'component',
        component: {
          ref: ChannelCreateModal,
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

  function onQrHandler(qrTitle: string, qrValue: string, qrLogo: string = '') {
    modalStore.close()
    modalStore.trigger({
      type: 'component',
      component: {
        ref: QrModal,
        props: {
          qrTitle,
          qrValue,
          qrLogo
        }
      }
    })
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(async function () {
      myInfo = await myState.agent.subscribeProfile()
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
    <div class="size-40">
      <Avatar
        initials={display.name}
        src={display.image}
        border="border-4 border-panda/20"
        background={display.image ? '' : 'bg-panda'}
        fill="fill-white"
        width="w-40"
      />
    </div>
    <p class="flex flex-row items-center gap-1">
      <span class="font-bold">{display.name}</span>
      {#if display.username}
        <button
          class="flex flex-row items-center gap-2"
          onclick={() =>
            onQrHandler(
              display.name,
              `${APP_ORIGIN}/${display.username}`,
              display.image
            )}
        >
          <span class="text-neutral-500">@{display.username}</span>
          <span class="text-neutral-500 *:size-5"><IconQrCode /></span>
        </button>
      {/if}
    </p>
    {#if $userInfo.bio}
      <div class="content-markdown">
        {@html md.render($userInfo.bio)}
      </div>
    {/if}
    <p class="mt-2 flex flex-row items-center gap-1 text-sm text-neutral-500">
      <span>Principal: {shortId(display._id)}</span>
      <TextClipboardButton textValue={display._id} />
      <button
        class="flex flex-row items-center gap-2"
        onclick={() => onQrHandler('Principal ID', display._id)}
      >
        <span class="*:size-5"><IconQrCode /></span>
      </button>
    </p>
    {#if !isMe}
      <div class="mt-6 flex flex-row items-end gap-4">
        <button
          type="button"
          class="{isFowllowing
            ? 'variant-ghost-primary'
            : 'variant-filled'} group btn btn-sm w-32"
          disabled={followingSubmitting !== ''}
          onclick={() => onFollowHandler($userInfo.id, !isFowllowing)}
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
          onclick={() => onCreateChannelHandler($userInfo.id)}
        >
          <span>Message</span>
        </button>
        <a
          class="text-sm text-surface-500 underline"
          href="https://github.com/ldclabs/ic-panda/tree/main/src/ic_dmsg_minter"
          target="_blank">Mint $DMSG</a
        >
      </div>
    {/if}
    <div
      class="mt-6 flex w-full max-w-xl flex-col items-center justify-center gap-4"
    >
      {#each links as link}
        <LinkItem {link} {onQrHandler} />
      {/each}
    </div>
  </div>
{:else}
  <div class="mt-16 grid justify-center">
    <span class="text-panda *:size-10"><Loading /></span>
  </div>
{/if}
