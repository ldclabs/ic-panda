<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import {
    type ChannelECDHInput,
    type UpdateChannelInput
  } from '$lib/canisters/messagechannel'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconAdminLine from '$lib/components/icons/IconAdminLine.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconEditLine from '$lib/components/icons/IconEditLine.svelte'
  import IconExchange2Line from '$lib/components/icons/IconExchange2Line.svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import {
    type ChannelInfoEx,
    type DisplayUserInfoEx,
    type MyMessageState
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { errMessage } from '$lib/types/result'
  import { sleep } from '$lib/utils/helper'
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
  import ChannelEditModel from './ChannelEditModel.svelte'
  import ChannelTopupModel from './ChannelTopupModel.svelte'
  import UserSelectModel from './UserSelectModel.svelte'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>
  export let channelInfo: ChannelInfoEx

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const members: Writable<DisplayUserInfoEx[]> = writable([])
  const myID = $myInfo.id.toText()
  const { canister, id } = channelInfo

  let mute = channelInfo.my_setting.mute
  let isManager = false
  let kekStatus = 0

  $: {
    if (channelInfo.my_setting.ecdh_remote.length > 0) {
      kekStatus = 1 // should accept the key
    } else if (channelInfo.my_setting.ecdh_pub.length > 0) {
      kekStatus = 2 // should wait for the key
    } else if (channelInfo._kek && !channelInfo._invalidKEK) {
      kekStatus = 3 // key exists
    } else {
      kekStatus = 0
    }
  }

  $: hasExchangeKeys =
    channelInfo.ecdh_request.filter(
      (r) => r[0].toText() !== myID && r[1][1].length === 0
    ).length > 0

  async function loadMembers() {
    const res = await myState.channelMembers(channelInfo, $myInfo)
    isManager = channelInfo._managers.includes(myID)
    members.set(res)
  }

  function onClickEditChannel() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelEditModel,
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
        ref: ChannelTopupModel,
        props: {
          myState,
          channel: channelInfo
        }
      }
    })
  }

  let muteSubmitting = false
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

  let myECDHSubmitting = false
  function onClickMyECDH() {
    myECDHSubmitting = true

    toastRun(async (signal: AbortSignal) => {
      if (channelInfo.my_setting.ecdh_remote.length === 1) {
        try {
          await myState.acceptKEK(channelInfo)
          await myState.decryptChannelDEK(channelInfo)
        } catch (err: any) {
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

  let leavingWord = ''
  let myLeavingSubmitting = false
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

  let adminExchangeKeysSubmitting = false
  function onClickAdminExchangeKeys() {
    adminExchangeKeysSubmitting = true
    toastRun(async (signal: AbortSignal) => {
      // fetch the latest ECDH request
      channelInfo = await myState.refreshChannel(channelInfo)
      await myState.adminExchangeKEK(channelInfo)
      channelInfo = await myState.refreshChannel(channelInfo)
      await loadMembers()
    }, toastStore).finally(() => {
      adminExchangeKeysSubmitting = false
    })
  }

  let adminAddManagersSubmitting = false
  function onClickAdminAddManagers() {
    const existsMembers = channelInfo.members.map((m) => m.toText())
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModel,
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

  let adminAddMembersSubmitting = false
  function onClickAdminAddMembers() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModel,
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

  let adminRemoveMembersSubmitting = ''
  function onClickAdminRemoveMember(_id: string) {
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
      channelInfo = await myState.refreshChannel(channelInfo, true)
      await loadMembers()
    }, toastStore)

    return abort
  })
</script>

<div
  class="h-[calc(100dvh-120px)] items-start overflow-y-auto bg-surface-500/5 pb-10 md:h-[calc(100dvh-60px)]"
>
  <section class="mt-4 flex w-full flex-row items-center gap-4 self-start px-4">
    <Avatar
      initials={channelInfo.name}
      src={channelInfo.image}
      border="border-4 border-white"
      background={channelInfo.image ? '' : 'bg-panda'}
      fill="fill-white"
      width="w-16"
    />
    <div class="flex-1">
      <p class="flex flex-row">
        <span>{channelInfo.name}</span>
        {#if isManager}
          <button
            type="button"
            class="btn ml-2 p-0 text-neutral-500"
            on:click={onClickEditChannel}
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
      <span class="text-sm font-normal text-neutral-500">Gas Balance:</span>
      <span class="font-bold text-panda">{channelInfo.gas}</span>
    </div>
    <button
      type="button"
      class="btn btn-sm hover:variant-soft-primary"
      on:click={onClickTopupChannel}
    >
      <span class="*:size-4"><IconAdd /></span>
      <span>Topup</span>
    </button>
  </section>
  <section class="mt-4 space-y-2 px-4">
    <div class="mb-2 text-sm opacity-50"><span>My Setting</span></div>
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
      <p>Request encryption key:</p>
      {#if kekStatus === 1}
        <button
          type="button"
          class="variant-filled-success btn btn-sm"
          on:click={onClickMyECDH}
          disabled={myECDHSubmitting}
          ><span>Key received, accept it</span></button
        >
      {:else if kekStatus === 2}
        <span class="text-sm opacity-50"
          >Request sent, waiting for a manager share key</span
        >
      {:else if kekStatus === 3}
        <span class="text-sm opacity-50"
          >Key already exists, no action needed</span
        >
      {:else}
        <button
          type="button"
          class="variant-filled-error btn btn-sm"
          on:click={onClickMyECDH}
          disabled={myECDHSubmitting}
          ><span>No key to decrypt messages, make a request</span></button
        >
      {/if}
      <span class="text-panda *:size-4 {myECDHSubmitting ? '' : 'invisible'}">
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
          placeholder="channel name"
        />
        <button
          type="button"
          class="variant-filled-warning !px-2 disabled:variant-filled-surface"
          on:click={onClickMyLeaving}
          disabled={myLeavingSubmitting ||
            leavingWord.trim() != channelInfo.name}
          ><span class="*:size-5">
            {#if myLeavingSubmitting}
              <IconCircleSpin />
            {:else}
              <IconLogout />
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
        >You are the only manager. Leaving the channel will delete all its data.</p
      >
    {/if}
  </section>
  <section class="mt-4 px-4">
    <div class="mb-2 items-center sm:grid sm:grid-cols-[1fr_auto]">
      <span class="text-sm opacity-50">Members</span>
      {#if isManager}
        <div class="flex flex-col space-x-1 sm:flex-row">
          <button
            type="button"
            class="btn btn-sm hover:variant-soft-primary"
            on:click={onClickAdminAddManagers}
            disabled={adminAddManagersSubmitting}
          >
            <span class="*:size-4"><IconAdd /></span>
            <span>Managers</span>
          </button>
          <button
            type="button"
            class="btn btn-sm hover:variant-soft-primary"
            on:click={onClickAdminAddMembers}
            disabled={adminAddMembersSubmitting}
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
            on:click={onClickAdminExchangeKeys}
            disabled={adminExchangeKeysSubmitting || !hasExchangeKeys}
          >
            <span class="*:size-4"><IconExchange2Line /></span>
            <span>Exchange keys</span>
            <span
              class="text-panda *:size-4 {adminExchangeKeysSubmitting
                ? ''
                : 'invisible'}"
            >
              <IconCircleSpin />
            </span>
          </button>
        </div>
      {/if}
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
                on:click={() => onClickAdminRemoveMember(member._id)}
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
