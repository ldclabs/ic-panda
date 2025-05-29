<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type ChannelECDHInput,
    type UpdateChannelInput
  } from '$lib/canisters/messagechannel'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconAdminLine from '$lib/components/icons/IconAdminLine.svelte'
  import IconCameraLine from '$lib/components/icons/IconCameraLine.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import IconExchange2Line from '$lib/components/icons/IconExchange2Line.svelte'
  import IconLogoutCircleRLine from '$lib/components/icons/IconLogoutCircleRLine.svelte'
  import {
    type ChannelInfoEx,
    type DisplayUserInfoEx,
    type MyMessageState
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { errMessage } from '$lib/types/result'
  import { getBytesString, getShortNumber, sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import { Principal } from '@dfinity/principal'
  import {
    Avatar,
    SlideToggle,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import { onMount } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'
  import ChannelEditModal from './ChannelEditModal.svelte'
  import ChannelImageUploadModal from './ChannelImageUploadModal.svelte'
  import ChannelTopupModal from './ChannelTopupModal.svelte'
  import ChannelUpdateStorageModal from './ChannelUpdateStorageModal.svelte'
  import UserSelectModal from './UserSelectModal.svelte'

  interface Props {
    myState: MyMessageState
    myInfo: Readable<UserInfo>
    channelInfo: ChannelInfoEx
    close: () => void
  }

  let { myState, myInfo, close, channelInfo = $bindable() }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const members: Writable<DisplayUserInfoEx[]> = writable([])
  const myID = $myInfo.id.toText()

  // svelte-ignore state_referenced_locally
  const { canister, id } = channelInfo
  // svelte-ignore state_referenced_locally
  let mute = $state(channelInfo.my_setting.mute)
  let isManager = $state(false)
  let validKEK = $state(true)

  let files_state = $derived(
    channelInfo.files_state[0] || {
      files_size_total: 0n,
      file_max_size: 0n,
      files_total: 0n
    }
  )
  let hasExchangeKeys = $derived(
    channelInfo.ecdh_request.filter(
      (r) => r[0].toText() !== myID && r[1][1].length === 0
    ).length > 0
  )

  async function loadMembers() {
    const res = await myState.channelMembers(channelInfo, $myInfo)
    isManager = channelInfo._managers.includes(myID)
    members.set(res)
  }

  function onUploadChannelImage() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelImageUploadModal,
        props: {
          myState,
          channel: channelInfo,
          onCompleted: async () => {
            channelInfo = await myState.refreshChannel(channelInfo)
          }
        }
      }
    })
  }

  function onClickEditChannel() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelEditModal,
        props: {
          channel: channelInfo,
          onSave: (input: UpdateChannelInput) => {
            return toastRun(async (signal: AbortSignal) => {
              const api = myState.api.channelAPI(canister)
              await api.update_channel(input)
              channelInfo = await myState.refreshChannel(channelInfo)
            }, toastStore).finally()
          }
        }
      }
    })
  }

  function onClickTopupChannel() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelTopupModal,
        props: {
          myState,
          channel: channelInfo
        }
      }
    })
  }

  function onClickUpdateChannelStorage() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelUpdateStorageModal,
        props: {
          myState,
          channel: channelInfo,
          onCompleted: async () => {
            channelInfo = await myState.refreshChannel(channelInfo)
          }
        }
      }
    })
  }

  let muteSubmitting = $state(false)
  function onClickMute() {
    muteSubmitting = true

    toastRun(async (signal: AbortSignal) => {
      const api = myState.api.channelAPI(canister)
      await sleep(1000)
      await api.update_my_setting({
        id,
        ecdh: [],
        mute: [mute],
        last_read: []
      })
      channelInfo = await myState.refreshChannel(channelInfo)
    }, toastStore).finally(() => {
      muteSubmitting = false
    })
  }

  let myECDHSubmitting = $state(false)
  function onClickMyECDH() {
    myECDHSubmitting = true

    toastRun(async (signal: AbortSignal) => {
      if (channelInfo.my_setting.ecdh_remote.length === 1) {
        try {
          await myState.acceptKEK(channelInfo)
          await myState.decryptChannelDEK(channelInfo)
          validKEK = true
          close()
        } catch (err: any) {
          validKEK = false
          toastStore.trigger({
            timeout: 10000,
            hideDismiss: false,
            background: 'variant-soft-error',
            message: `Failed to receive the key. A new key has been requested.\n<br />Error: ${errMessage(err)}`
          })
          const my_setting = { ...channelInfo.my_setting }
          my_setting.ecdh_remote = []
          my_setting.ecdh_pub = []
          await myState.requestKEK({ ...channelInfo, my_setting })
        }
      } else {
        await myState.requestKEK(channelInfo)
      }

      channelInfo = await myState.refreshChannel(channelInfo)
    }, toastStore).finally(() => {
      myECDHSubmitting = false
    })
  }

  let leavingWord = $state('')
  let myLeavingSubmitting = $state(false)
  function onClickMyLeaving() {
    if (leavingWord.trim() != channelInfo.name) {
      return
    }

    myLeavingSubmitting = true
    toastRun(async (signal: AbortSignal) => {
      const api = myState.api.channelAPI(canister)
      await api.leave_channel(id, true)
      await myState.removeChannel(canister, id)

      goto('/_/messages')
    }, toastStore).finally(() => {
      myLeavingSubmitting = false
    })
  }

  let adminExchangeKeysSubmitting = $state(false)
  function onClickAdminExchangeKeys() {
    adminExchangeKeysSubmitting = true
    toastRun(async (signal: AbortSignal) => {
      // fetch the latest ECDH request
      await myState.refreshChannel(channelInfo)
      await myState.adminExchangeKEK(channelInfo)
      channelInfo = await myState.refreshChannel(channelInfo)
      await loadMembers()
    }, toastStore).finally(() => {
      adminExchangeKeysSubmitting = false
    })
  }

  let adminAddManagersSubmitting = $state(false)
  function onClickAdminAddManagers() {
    const existsMembers = channelInfo.members.map((m) => m.toText())
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModal,
        props: {
          isAddManager: true,
          existsManagers: channelInfo._managers,
          existsMembers,
          myState: myState,
          onSave: (members: Array<[Principal, Uint8Array | null]>) => {
            adminAddManagersSubmitting = true
            toastRun(async (signal: AbortSignal) => {
              // adminAddManager
              const member = members[0]
              if (
                member &&
                members.length === 1 &&
                existsMembers.includes(member[0].toText())
              ) {
                await myState.adminAddManager(channelInfo, member[0])
              } else {
                await myState.adminAddMembers(channelInfo, 'Manager', members)
              }
              channelInfo = await myState.refreshChannel(channelInfo)
              await loadMembers()
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
          existsManagers: channelInfo._managers,
          existsMembers: channelInfo.members.map((m) => m.toText()),
          myState: myState,
          onSave: (members: Array<[Principal, Uint8Array | null]>) => {
            adminAddMembersSubmitting = true
            toastRun(async (signal: AbortSignal) => {
              await myState.adminAddMembers(channelInfo, 'Member', members)
              channelInfo = await myState.refreshChannel(channelInfo)
              await loadMembers()
            }, toastStore).finally(() => {
              adminAddMembersSubmitting = false
            })
          }
        }
      }
    })
  }

  let adminRemoveMembersSubmitting = $state('')
  function onClickAdminRemoveMember(_id: string) {
    adminRemoveMembersSubmitting = _id
    toastRun(async (signal: AbortSignal) => {
      const api = myState.api.channelAPI(canister)
      await api.remove_member({
        id,
        member: Principal.fromText(_id),
        ecdh: {
          ecdh_remote: [],
          ecdh_pub: []
        } as ChannelECDHInput
      })

      channelInfo = await myState.refreshChannel(channelInfo)
      await loadMembers()
    }, toastStore).finally(() => {
      adminRemoveMembersSubmitting = ''
    })
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      channelInfo = await myState.refreshChannel(channelInfo)
      try {
        await myState.decryptChannelDEK(channelInfo)
        validKEK = true
      } catch (err) {
        validKEK = false
        console.error('Failed to decrypt channel DEK', channelInfo._kek, err)
      }
      await loadMembers()
    }, toastStore)

    return abort
  })
</script>

<div
  class="h-[calc(100dvh-120px)] items-start overflow-y-auto bg-surface-500/5 pb-10 md:h-[calc(100dvh-60px)]"
>
  {#if channelInfo.my_setting.ecdh_remote.length > 0}
    <section class="mt-4 px-4">
      <button
        type="button"
        class="variant-filled-success btn w-full"
        onclick={onClickMyECDH}
        disabled={myECDHSubmitting}
      >
        <span>Activate key to start chatting</span>
        {#if myECDHSubmitting}
          <span class=" text-panda *:size-4">
            <IconCircleSpin />
          </span>
        {/if}
      </button>
    </section>
  {/if}
  <section class="mt-4 flex w-full flex-row items-center gap-4 self-start px-4">
    {#if isManager}
      <button
        class="group btn relative p-0 hover:bg-surface-500/50"
        onclick={onUploadChannelImage}
      >
        <Avatar
          initials={channelInfo.name}
          src={channelInfo.image}
          border="border-4 border-white"
          background={channelInfo.image ? '' : 'bg-panda'}
          fill="fill-white"
          width="size-20"
        />
        <span
          class="invisible absolute left-1/2 top-1/2 !ml-0 -translate-x-1/2 -translate-y-1/2 text-surface-500 transition-all *:size-6 group-hover:visible"
          ><IconCameraLine /></span
        >
      </button>
    {:else}
      <Avatar
        initials={channelInfo.name}
        src={channelInfo.image}
        border="border-4 border-white"
        background={channelInfo.image ? '' : 'bg-panda'}
        fill="fill-white"
        width="size-20"
      />
    {/if}
    <div class="flex-1">
      <p class="flex flex-row">
        <span>{channelInfo.name}</span>
        {#if isManager}
          <button
            type="button"
            class="btn ml-2 p-0 text-neutral-500"
            onclick={onClickEditChannel}
          >
            <span class="*:size-4"><IconEditLine /></span>
          </button>
        {/if}
      </p>
      {#if channelInfo.description}
        <div class="mt-2">
          {@html md.render(channelInfo.description)}
        </div>
      {/if}
    </div>
  </section>
  <section class="mt-2 flex flex-row gap-2 px-4 max-sm:flex-col">
    <div class="flex flex-row items-center gap-1">
      <span class="text-sm font-normal text-neutral-500">Messages:</span>
      <span class="font-bold text-panda"
        >{channelInfo.latest_message_id - channelInfo.message_start + 1}</span
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      <span class="text-sm font-normal text-neutral-500">Gas balance:</span>
      <span class="font-bold text-panda">{getShortNumber(channelInfo.gas)}</span
      >
    </div>
    <button
      type="button"
      class="btn btn-sm hover:variant-soft-primary"
      onclick={onClickTopupChannel}
    >
      <span class="*:size-4"><IconAdd /></span>
      <span>Topup</span>
    </button>
  </section>
  <section class="mt-0 flex flex-row gap-2 px-4 max-sm:flex-col">
    <div class="flex flex-row items-center gap-1">
      <span class="text-sm font-normal text-neutral-500">Files:</span>
      <span class="font-bold text-panda">{files_state.files_total}</span>
    </div>
    <div class="flex flex-row items-center gap-2">
      <span class="text-sm font-normal text-neutral-500">Total size:</span>
      <span class="font-bold text-panda"
        >{getBytesString(files_state.files_size_total)}</span
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      <span class="text-sm font-normal text-neutral-500">Max file size:</span>
      <span class="font-bold text-panda"
        >{getBytesString(files_state.file_max_size)}</span
      >
    </div>
    <button
      type="button"
      class="btn btn-sm hover:variant-soft-primary"
      disabled={!isManager}
      onclick={onClickUpdateChannelStorage}
    >
      <span class="*:size-4"><IconEditLine /></span>
      <span>Update</span>
    </button>
  </section>
  <section class="mt-4 space-y-2 px-4">
    <div class="mb-2 text-sm opacity-50"><span>My settings</span></div>
    <div class="flex flex-row items-center gap-4">
      <p>Access key:</p>
      {#if channelInfo.my_setting.ecdh_remote.length > 0}
        <button
          type="button"
          class="variant-filled-success btn btn-sm"
          onclick={onClickMyECDH}
          disabled={myECDHSubmitting}><span>Activate Key</span></button
        >
      {:else if channelInfo.my_setting.ecdh_pub.length > 0}
        <span class="text-sm opacity-50">Pending Approval</span>
      {:else if validKEK}
        <span class="text-sm opacity-50">Ready to Use</span>
      {:else}
        <button
          type="button"
          class="variant-filled-error btn btn-sm"
          onclick={onClickMyECDH}
          disabled={myECDHSubmitting}><span>Request Access</span></button
        >
      {/if}
      <span class="text-panda *:size-4 {myECDHSubmitting ? '' : 'invisible'}">
        <IconCircleSpin />
      </span>
    </div>
    <div class="flex flex-row items-center gap-4">
      <p>Mute notifications:</p>
      <SlideToggle
        name="setting-mute"
        active="bg-panda"
        size="sm"
        bind:checked={mute}
        on:click={onClickMute}
        disabled={muteSubmitting}
      />
      <span class="text-panda *:size-4 {muteSubmitting ? '' : 'invisible'}">
        <IconCircleSpin />
      </span>
    </div>
    <div class="flex flex-row items-center gap-4">
      <p>Leave channel:</p>
      <div
        class="input-group w-full max-w-60 grid-cols-[1fr_auto] bg-surface-500/5"
      >
        <input
          type="text"
          class="border-gray/10 h-8 truncate py-1 leading-8 invalid:input-warning"
          bind:value={leavingWord}
          onfocus={() => (leavingWord = channelInfo.name)}
          placeholder="channel name"
        />
        <button
          type="button"
          class="variant-filled-error !px-2 disabled:variant-filled-surface"
          onclick={onClickMyLeaving}
          disabled={myLeavingSubmitting ||
            leavingWord.trim() != channelInfo.name}
          ><span class="*:size-5">
            {#if myLeavingSubmitting}
              <IconCircleSpin />
            {:else}
              <IconLogoutCircleRLine />
            {/if}
          </span></button
        >
      </div>
    </div>
    {#if isManager && channelInfo._managers.length < 2}
      <p
        class="h-5 text-sm text-error-500 {leavingWord === channelInfo.name
          ? 'visible'
          : 'invisible'}"
        >You're the only manager of this channel. If you leave, all channel data
        will be permanently deleted.</p
      >
    {/if}
  </section>
  <section class="mt-4 px-4">
    <div class="mb-2 items-center sm:grid sm:grid-cols-[1fr_auto]">
      <span class="text-sm opacity-50">Members</span>
      <div class="flex flex-col space-x-1 sm:flex-row">
        <button
          type="button"
          class="btn btn-sm hover:variant-soft-primary"
          onclick={onClickAdminAddManagers}
          disabled={!isManager ||
            adminAddManagersSubmitting ||
            channelInfo.managers.length >= 5}
        >
          <span class="*:size-4"><IconAdd /></span>
          <span>Managers</span>
        </button>
        <button
          type="button"
          class="btn btn-sm hover:variant-soft-primary"
          onclick={onClickAdminAddMembers}
          disabled={!isManager ||
            adminAddMembersSubmitting ||
            channelInfo.members.length >= 995}
        >
          <span class="*:size-4"><IconAdd /></span>
          <span>Members</span>
        </button>
        <button
          type="button"
          class="btn btn-sm hover:variant-soft-primary {hasExchangeKeys &&
          !adminExchangeKeysSubmitting
            ? 'text-panda'
            : 'text-neutral-500'}"
          onclick={onClickAdminExchangeKeys}
          disabled={!isManager ||
            adminExchangeKeysSubmitting ||
            !hasExchangeKeys}
        >
          <span class="*:size-4"><IconExchange2Line /></span>
          <span>Approve Requests</span>
          <span
            class="text-panda *:size-4 {adminExchangeKeysSubmitting
              ? ''
              : 'invisible'}"
          >
            <IconCircleSpin />
          </span>
        </button>
      </div>
    </div>
    <div class="!mt-0">
      {#each $members as member (member._id)}
        <div class="group grid grid-cols-[1fr_auto] items-center py-1">
          <div class="flex flex-row items-center space-x-2">
            <Avatar
              initials={member.name}
              src={member.image}
              fill="fill-white"
              width="w-10"
              border={member._id === myID ? 'border-2 border-panda' : ''}
            />
            <span class="ml-1">{member.name}</span>
            {#if member.username}
              <span class="text-neutral-500">@{member.username}</span>
            {/if}
            {#if member.is_manager}
              <span class="text-neutral-500 *:size-4"><IconAdminLine /></span>
            {/if}
            {#if member.ecdh_request === 1}
              <span class="variant-ringed-primary badge text-neutral-500"
                >Request key</span
              >
            {:else if member.ecdh_request === 2}
              <span class="variant-ringed-surface badge text-neutral-500"
                >Key filled</span
              >
            {/if}
          </div>
          <div class="flex flex-row items-center space-x-2">
            {#if isManager && !member.is_manager}
              <button
                class="variant-soft-warning btn btn-sm invisible group-hover:visible"
                type="button"
                disabled={adminRemoveMembersSubmitting !== ''}
                onclick={() => onClickAdminRemoveMember(member._id)}
              >
                <span>Remove</span>
              </button>
              <span
                class="text-panda *:size-4 {adminRemoveMembersSubmitting ===
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
  </section>
</div>
