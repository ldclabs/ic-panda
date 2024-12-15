<script lang="ts">
  import { page } from '$app/stores'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type Link,
    type ProfileInfo,
    type UpdateProfileInput
  } from '$lib/canisters/messageprofile'
  import { type Delegator } from '$lib/canisters/nameidentity'
  import AvatarUploadModal from '$lib/components/core/AvatarUploadModal.svelte'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconAdminLine from '$lib/components/icons/IconAdminLine.svelte'
  import IconCameraLine from '$lib/components/icons/IconCameraLine.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import IconUserForbidLine from '$lib/components/icons/IconUserForbidLine.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextClipboardButton from '$lib/components/ui/TextClipboardButton.svelte'
  import { APP_ORIGIN } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import {
    toDisplayUserInfo,
    type DisplayUserInfo,
    type MyMessageState
  } from '$lib/stores/message'
  import { MessageAgent } from '$lib/stores/message_agent'
  import { ErrorLogs, toastRun } from '$lib/stores/toast'
  import { errMessage, unwrapOption } from '$lib/types/result'
  import { shortId } from '$lib/utils/auth'
  import { sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import { isNotificationSupported } from '$lib/utils/window'
  import LinkItem from '$src/lib/components/ui/LinkItem.svelte'
  import { Principal } from '@dfinity/principal'
  import {
    Avatar,
    getModalStore,
    getToastStore,
    LightSwitch,
    SlideToggle
  } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'
  import ActivateUsernameAccountModal from './ActivateUsernameAccountModal.svelte'
  import ChannelCreateModal from './ChannelCreateModal.svelte'
  import LinkEditModal from './LinkEditModal.svelte'
  import ProfileEditModal from './ProfileEditModal.svelte'
  import UserRegisterModal from './UserRegisterModal.svelte'
  import UserSelectModal from './UserSelectModal.svelte'

  interface Props {
    myState: MyMessageState
  }

  let { myState }: Props = $props()

  type DisplayUserInfoEx = DisplayUserInfo & {
    role: number
    sign_in_at: number
  }

  // local: gnhwq-7p3rq-chahe-22f7s-btty6-ntken-g6dff-xwbyd-4qfse-37euh-5ae
  const PandaID = Principal.fromText(
    '77ibd-jp5kr-moeco-kgoar-rro5v-5tng4-krif5-5h2i6-osf2f-2sjtv-kqe'
  )

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const myFollowing: Writable<DisplayUserInfo[]> = writable([])
  const srcIdentity = authStore.srcIdentity?.getPrincipal().toText() || ''

  let myInfo: Readable<(UserInfo & ProfileInfo) | null> | undefined = $state()

  let grantedNotification = $state(
    isNotificationSupported && Notification.permission === 'granted'
  )
  let displayDebug = $state(false)

  let pathname = $derived($page?.url?.pathname || '')
  let links = $derived($myInfo?.links || [])
  let myUsername = $derived($myInfo?.username[0] || '')
  let isUsernameAccount = $state(!!authStore.identity?.username)
  let usernameAccount = $state('')
  let delegators: Delegator[] = $state([])
  let delegatorsInfo: DisplayUserInfoEx[] = $state([])
  let isManager = $derived(
    srcIdentity != '' &&
      delegators.some(
        (delegator) =>
          delegator.role == 1 && delegator.owner.toText() == srcIdentity
      )
  )

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

  async function loadDelegators() {
    delegators = await authStore.nameIdentityAPI
      .get_delegators(authStore.identity?.username || myUsername)
      .catch(() => [])
    if (!isUsernameAccount) {
      return
    }

    const res: DisplayUserInfoEx[] = []
    const users = await myState.batchLoadUsersInfo(
      delegators.map((delegator) => delegator.owner)
    )
    const userInfos = users.map(toDisplayUserInfo)
    for (const user of delegators) {
      const _id = user.owner.toText()
      const info = userInfos.find(
        (info) => info._id === _id
      ) as DisplayUserInfoEx
      if (info) {
        info.role = user.role
        info.sign_in_at = Number(user.sign_in_at)
        res.push(info)
      } else {
        res.push({
          _id,
          username: '',
          name: 'Unknown',
          image: '',
          role: user.role,
          sign_in_at: Number(user.sign_in_at)
        })
      }
    }
    delegatorsInfo = res
  }

  async function saveProfile(profile: UserInfo & ProfileInfo) {
    if (profile.name && profile.name !== $myInfo?.name) {
      const user = await myState.api.update_my_name(profile.name)
      await myState.agent.setUser(user)
    }

    const bio = profile.bio.trim()
    if (bio !== $myInfo?.bio) {
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
    if ($myInfo?.username.length == 1) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: ProfileEditModal,
          props: {
            myState,
            myInfo,
            onSave: saveProfile
          }
        }
      })
    } else {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModal,
          props: {
            myState
          }
        }
      })
    }
  }

  function onUploadAvatarHandler() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: AvatarUploadModal,
        props: {
          myState
        }
      }
    })
  }

  let followingSubmitting = $state('')
  function onFollowHandler(user: Principal, fowllowing: boolean = true) {
    toastRun(async () => {
      if (myState.principal.isAnonymous()) {
        await authStore.signIn()
      } else if (!$myInfo) {
        modalStore.trigger({
          type: 'component',
          component: {
            ref: UserRegisterModal,
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
      await authStore.signIn()
    } else if (!$myInfo) {
      modalStore.trigger({
        type: 'component',
        component: {
          ref: UserRegisterModal,
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

  let editLinkSubmitting = $state(-1)
  async function onEditLink(link: Link | null = null, index: number = -1) {
    const _links = [...links]
    const profile = await myState.agent.getProfile()
    editLinkSubmitting = index
    modalStore.trigger({
      type: 'component',
      component: {
        ref: LinkEditModal,
        props: {
          link,
          onSave: async (link: Link) => {
            if (index >= 0) {
              _links[index] = link
            } else {
              _links.push(link)
            }
            await myState.agent.profileAPI.update_links(_links)
            await myState.agent.setProfile({
              ...profile,
              links: _links
            })
            editLinkSubmitting = -1
          }
        }
      }
    })
  }

  let deleteLinkSubmitting = $state(-1)
  async function onDeleteLink(index: number) {
    deleteLinkSubmitting = index
    const _links = [...links.slice(0, index), ...links.slice(index + 1)]
    const profile = await myState.agent.getProfile()
    await myState.agent.profileAPI.update_links(_links)
    await myState.agent.setProfile({
      ...profile,
      links: _links
    })
    deleteLinkSubmitting = -1
  }

  let adminAddManagersSubmitting = $state(false)
  function onClickAdminAddManagers() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModal,
        props: {
          isAddManager: true,
          existsManagers: delegators
            .filter((delegator) => delegator.role == 1)
            .map((delegator) => delegator.owner.toText()),
          existsMembers: [],
          myState: myState,
          onSave: (members: Array<[Principal, Uint8Array | null]>) => {
            adminAddManagersSubmitting = true
            toastRun(async (signal: AbortSignal) => {
              for (const [id, _] of members) {
                await authStore.nameIdentityAPI.add_delegator(
                  authStore.identity?.username || myUsername,
                  id,
                  1
                )
              }

              await loadDelegators()
            }, toastStore).finally(() => {
              adminAddManagersSubmitting = false
            })
          }
        }
      }
    })
  }

  let adminAddMembersSubmitting = $state(false)
  function onClickAdminAddMembers() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModal,
        props: {
          isAddManager: false,
          existsManagers: delegators
            .filter((delegator) => delegator.role == 1)
            .map((delegator) => delegator.owner.toText()),
          existsMembers: delegators
            .filter((delegator) => delegator.role < 1)
            .map((delegator) => delegator.owner.toText()),
          myState: myState,
          onSave: (members: Array<[Principal, Uint8Array | null]>) => {
            adminAddMembersSubmitting = true
            toastRun(async (signal: AbortSignal) => {
              for (const [id, _] of members) {
                await authStore.nameIdentityAPI.add_delegator(
                  authStore.identity?.username || myUsername,
                  id,
                  0
                )
              }

              await loadDelegators()
            }, toastStore).finally(() => {
              adminAddMembersSubmitting = false
            })
          }
        }
      }
    })
  }

  let adminRemoveDelegatorsSubmitting = $state('')
  function onClickAdminRemoveMember(id: string) {
    adminRemoveDelegatorsSubmitting = id
    toastRun(async (signal: AbortSignal) => {
      await authStore.nameIdentityAPI.remove_delegator(
        authStore.identity?.username || myUsername,
        Principal.fromText(id)
      )
      await loadDelegators()
    }, toastStore).finally(() => {
      adminRemoveDelegatorsSubmitting = ''
    })
  }

  function onActivateUsernameAccountHandler() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ActivateUsernameAccountModal,
        props: {
          username: myUsername,
          usernameAccount
        }
      }
    })
  }

  let clearCachedMessagesSubmitting = $state(false)
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
      if (myUsername || isUsernameAccount) {
        loadDelegators()
      }

      if (myUsername) {
        usernameAccount = (
          await authStore.nameIdentityAPI.get_principal(myUsername)
        ).toText()
      }
    }, toastStore)

    onfinally(async () => {
      await tick()

      const perm = await myState.agent.getLocal<string>(
        MessageAgent.KEY_NOTIFY_PERM
      )
      if (perm === 'denied') {
        grantedNotification = false
      }
      await loadMyFollowing()
    })
    return abort
  })

  function errorLogsText(logs: Error[]): string {
    return logs.map((log) => `${errMessage(log)}\n${log.stack}`).join('\n')
  }
</script>

{#if $myInfo}
  {@const display = toDisplayUserInfo($myInfo)}
  <section class="h-[calc(100dvh-60px)] w-full overflow-y-auto md:h-dvh">
    <div
      class="mx-auto flex min-h-full w-full max-w-3xl flex-col items-center gap-1 p-8 pb-12"
    >
      <button
        class="group btn relative p-0 hover:bg-surface-500/50"
        onclick={onUploadAvatarHandler}
      >
        <Avatar
          initials={display.name}
          src={display.image}
          border="border-4 border-panda/20"
          background={display.image ? '' : 'bg-panda'}
          fill="fill-white"
          width="size-40"
        />
        <span
          class="invisible absolute left-1/2 top-1/2 !ml-0 -translate-x-1/2 -translate-y-1/2 text-surface-500 transition-all *:size-6 group-hover:visible"
          ><IconCameraLine /></span
        >
      </button>
      <p class="relative space-x-1">
        <span class="font-bold">{display.name}</span>
        {#if display.username}
          {@const link = `${APP_ORIGIN}/${display.username}`}
          <a
            class="text-neutral-500 underline underline-offset-4 hover:text-surface-900-50-token"
            href={link}>@{display.username}</a
          >
          <TextClipboardButton textValue={link} />
        {/if}
        <button
          type="button"
          class="btn absolute right-[-28px] top-1 p-0 text-neutral-500 hover:text-surface-900-50-token"
          onclick={onMeHandler}
        >
          <span class="*:size-5"><IconEditLine /></span>
        </button>
      </p>
      {#if $myInfo.bio}
        <div class="content-markdown">
          {@html md.render($myInfo.bio)}
        </div>
      {/if}
      <p class="mt-2 flex flex-row items-center gap-1 text-sm text-neutral-500">
        <span>Principal: {shortId(display._id)}</span>
        <TextClipboardButton textValue={display._id} />
      </p>
      <div
        class="mt-6 flex w-full max-w-xl flex-col items-center justify-center gap-4"
      >
        {#each links as link, i}
          <div class="relative w-full pr-[72px]">
            <LinkItem {link} />
            <div
              class="absolute right-0 top-1/2 flex -translate-y-1/2 flex-row gap-1 text-neutral-500/50"
            >
              <button
                type="button"
                class="p-1 hover:text-surface-900-50-token"
                disabled={editLinkSubmitting !== -1}
                onclick={() => onEditLink(link, i)}
              >
                <span class="*:size-5">
                  {#if editLinkSubmitting === i}
                    <IconCircleSpin />
                  {:else}
                    <IconEditLine />
                  {/if}
                </span>
              </button>
              <button
                type="button"
                class="p-1 hover:text-surface-900-50-token"
                disabled={deleteLinkSubmitting !== -1}
                onclick={() => onDeleteLink(i)}
              >
                <span class="*:size-5">
                  {#if deleteLinkSubmitting === i}
                    <IconCircleSpin />
                  {:else}
                    <IconDeleteBin />
                  {/if}
                </span>
              </button>
            </div>
          </div>
        {/each}
        <button
          type="button"
          class="bg-surface-hover-token flex w-full flex-row items-center justify-center gap-2 rounded-lg border border-surface-500/10 px-2 py-4 text-neutral-500 hover:text-surface-900-50-token"
          onclick={() => onEditLink()}
        >
          <span class="*:size-5"><IconAdd /></span>
          <span>Add Link</span>
        </button>
      </div>
      {#if isUsernameAccount}
        <div class="mt-6 flex w-full flex-col gap-2">
          <div class="mb-2 items-center sm:grid sm:grid-cols-[1fr_auto]">
            <span class="text-sm opacity-50">Delegate Accounts</span>
            <div class="flex flex-col space-x-1 sm:flex-row">
              <button
                type="button"
                class="btn btn-sm hover:variant-soft-primary"
                onclick={onClickAdminAddManagers || adminAddManagersSubmitting}
                disabled={!isManager}
              >
                <span class="*:size-4"><IconAdd /></span>
                <span>Managers</span>
              </button>
              <button
                type="button"
                class="btn btn-sm hover:variant-soft-primary"
                onclick={onClickAdminAddMembers}
                disabled={!isManager || adminAddMembersSubmitting}
              >
                <span class="*:size-4"><IconAdd /></span>
                <span>Members</span>
              </button>
            </div>
          </div>

          <div class="flex flex-col">
            {#if !myUsername}
              <p class="mb-2">
                This is a <b>Username Permanent Account</b>, please transfer the
                username
                <span class="font-semibold text-primary-500"
                  >{authStore.identity!.username}</span
                > to this account.
              </p>
            {/if}
            {#each delegatorsInfo as member (member._id)}
              <div class="group grid grid-cols-[1fr_auto] items-center py-1">
                <div class="flex flex-row items-center space-x-2">
                  <Avatar
                    initials={member.name}
                    src={member.image}
                    fill="fill-white"
                    width="w-10"
                    border={member._id === srcIdentity
                      ? 'border-2 border-panda'
                      : ''}
                  />
                  <span class="ml-1">{member.name}</span>
                  {#if member.username}
                    <span class="text-neutral-500">@{member.username}</span>
                  {/if}
                  {#if member.role == 1}
                    <span class="text-neutral-500 *:size-4"
                      ><IconAdminLine /></span
                    >
                  {:else if member.role == -1}
                    <span class="text-neutral-500 *:size-4"
                      ><IconUserForbidLine /></span
                    >
                  {/if}
                </div>
                <div class="flex flex-row items-center space-x-2">
                  {#if isManager && member.role != 1}
                    <button
                      class="variant-soft-warning btn btn-sm invisible group-hover:visible"
                      type="button"
                      disabled={adminRemoveDelegatorsSubmitting !== ''}
                      onclick={() => onClickAdminRemoveMember(member._id)}
                    >
                      <span>Remove</span>
                    </button>
                    <span
                      class="text-panda *:size-4 {adminRemoveDelegatorsSubmitting ===
                      member._id
                        ? ''
                        : 'invisible'}"
                    >
                      <IconCircleSpin />
                    </span>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        </div>
      {:else if myUsername}
        <div class="mt-6 flex w-full flex-col gap-2">
          <div class="mb-2">
            <span class="text-sm opacity-50">Username Permanent Account</span>
            <button
              type="button"
              class="variant-filled-primary btn btn-sm ml-4 py-1"
              onclick={onActivateUsernameAccountHandler}
              disabled={delegators.length > 0}
            >
              <span>Activate</span>
            </button>
          </div>

          <div class="flex flex-col gap-4">
            <p>
              A <b>username permanent account</b> is a standalone fixed account generated
              from the username that does not change. This account supports adding
              multiple delegate accounts, allowing multiple users to use it simultaneously,
              making it ideal for team collaboration.
            </p>
            {#if delegators.length > 0}
              <p>
                The permanent account for <span
                  class="font-semibold text-primary-500">{myUsername}</span
                >
                has been activated, with the Principal ID:<br />
                <span class="font-semibold">{usernameAccount}</span>.<br />
                Please transfer the username to this account and switch to it in
                "More" menu for management.
              </p>
            {/if}
          </div>
        </div>
      {/if}
      <div class="mt-6 flex w-full flex-col gap-2">
        <div class="mb-2 text-sm opacity-50"
          ><button onclick={() => (displayDebug = !displayDebug)}>
            <span>My Setting</span>
          </button></div
        >
        <div class="flex flex-row items-center gap-4">
          <p>Dark mode:</p>
          <LightSwitch />
        </div>
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
              onclick={onClearCachedMessages}
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

      {#if $myFollowing.length > 0}
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
                      class="text-neutral-500 underline underline-offset-4"
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
                    onclick={() =>
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
                    class="variant-filled-primary btn btn-sm"
                    type="button"
                    onclick={() =>
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
      {#if pathname.startsWith('/_/profile') && myState.principal.compareTo(PandaID) !== 'eq'}
        <div
          class="mx-auto mt-24 flex flex-col items-center space-y-2 self-end text-sm"
        >
          <div class="flex flex-row items-center">
            <a
              class="flex flex-row items-center space-x-2"
              href="https://dmsg.net/PANDA"
            >
              <Avatar
                src="/_assets/logo_panda.png"
                fill="fill-white"
                width="w-8"
              />
              <span class="ml-1 truncate">ICPanda</span>
              <span class="text-neutral-500">@PANDA</span>
            </a>
            <button
              type="button"
              title="End-to-end encrypted message"
              class="variant-filled-primary btn btn-sm ml-2 w-32"
              onclick={() => onCreateChannelHandler(PandaID)}
            >
              <span>Message</span>
            </button>
          </div>
          <p class="text-neutral-500"
            >If you encounter any issues, please message us.</p
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
