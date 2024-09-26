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
  import { toastRun } from '$lib/stores/toast'
  import { errMessage } from '$lib/types/result'
  import { sleep } from '$lib/utils/helper'
  import { md } from '$lib/utils/markdown'
  import {
    type ChannelInfoEx,
    type DisplayUserInfoEx,
    type MyMessageState
  } from '$src/lib/stores/message'
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
              const api = await myState.api.channelAPI(canister)
              await api.update_channel(input)
              channelInfo = await myState.refreshChannel(channelInfo)
            }, toastStore).finally()
          }
        }
      }
    })
  }

  let muteSubmitting = false
  function onClickMute() {
    muteSubmitting = true

    toastRun(async (signal: AbortSignal) => {
      const api = await myState.api.channelAPI(canister)
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
    if (leavingWord != channelInfo.name) {
      return
    }

    myLeavingSubmitting = true
    toastRun(async (signal: AbortSignal) => {
      const api = await myState.api.channelAPI(canister)
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
    modalStore.trigger({
      type: 'component',
      component: {
        ref: UserSelectModel,
        props: {
          title: 'Add managers',
          existsManagers: channelInfo._managers,
          existsMembers: channelInfo.members.map((m) => m.toText()),
          myState: myState,
          onSave: (members: Array<[Principal, Uint8Array | null]>) => {
            adminAddManagersSubmitting = true
            toastRun(async (signal: AbortSignal) => {
              await myState.adminAddMembers(channelInfo, 'Manager', members)
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
          title: 'Add members',
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
      const api = await myState.api.channelAPI(canister)
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
      await loadMembers()
    }, toastStore)

    return abort
  })

  $: hasExchangeKeys =
    channelInfo.ecdh_request.filter(
      (r) => r[0].toText() !== myID && r[1][1].length === 0
    ).length > 0
</script>

<div
  class="h-[calc(100dvh-110px)] items-start overflow-y-auto bg-gray/5 pb-10 sm:h-[calc(100dvh-140px)]"
>
  <section class="mt-4 flex w-full flex-row items-center gap-4 self-start px-4">
    <Avatar
      initials={channelInfo.name}
      src={channelInfo.image}
      border="border-4 border-white"
      background="bg-panda"
      fill="fill-white"
      width="w-16"
    />
    <div class="flex-1">
      <p class="flex flex-row">
        <span>{channelInfo.name}</span>
        {#if isManager}
          <button
            type="button"
            class="btn ml-2 p-0 text-gray/60"
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
      <span class="text-sm font-normal text-gray/50">Messages:</span>
      <span class="font-bold text-panda"
        >{channelInfo.latest_message_id - channelInfo.message_start + 1}</span
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      <span class="text-sm font-normal text-gray/50">Gas Balance:</span>
      <span class="font-bold text-panda">{channelInfo.gas}</span>
    </div>
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
      {#if channelInfo.my_setting.ecdh_remote.length > 0}
        <button
          type="button"
          class="variant-ringed btn btn-sm text-panda"
          on:click={onClickMyECDH}
          disabled={myECDHSubmitting}
          ><span>Key received, accept it</span></button
        >
      {:else if channelInfo.my_setting.ecdh_pub.length > 0}
        <span class="text-sm opacity-50"
          >Request sent, waiting for a manager share key</span
        >
      {:else if channelInfo._kek}
        <span class="text-sm opacity-50"
          >Key already exists, no action needed</span
        >
        <button
          type="button"
          class="variant-soft-warning btn btn-sm hidden"
          on:click={onClickMyECDH}
          disabled={myECDHSubmitting}><span>Request again</span></button
        >
      {:else}
        <button
          type="button"
          class="variant-ringed btn btn-sm text-red-500"
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
        class="input-group input-group-divider w-64 grid-cols-[1fr_80px] bg-gray/5"
      >
        <input
          type="text"
          class="h-8 !w-36 truncate border-gray/10 py-1 leading-8 invalid:input-warning hover:bg-white/90"
          bind:value={leavingWord}
          placeholder="Enter channel name"
        />
        <button
          type="button"
          class="variant-filled-warning disabled:variant-filled-surface"
          on:click={onClickMyLeaving}
          disabled={myLeavingSubmitting || leavingWord != channelInfo.name}
          >Leave</button
        >
      </div>
      <span
        class="text-panda *:size-4 {myLeavingSubmitting ? '' : 'invisible'}"
      >
        <IconCircleSpin />
      </span>
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
              : 'text-gray/60'}"
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
    <div class="!mt-0 divide-y divide-gray/5">
      {#each $members as member (member._id)}
        <div class="grid grid-cols-[1fr_auto] items-center py-1">
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
              <span class="text-gray/60">@{member.username}</span>
            {/if}
            {#if member.is_manager}
              <span class="text-gray/60 *:size-4"><IconAdminLine /></span>
            {/if}
            {#if member.ecdh_request === 1}
              <span class="variant-ringed-primary badge text-gray/60"
                >Request key</span
              >
            {:else if member.ecdh_request === 2}
              <span class="variant-ringed-surface badge text-gray/60"
                >Key filled</span
              >
            {/if}
          </div>
          <div class="flex flex-row items-center space-x-2">
            {#if isManager && !member.is_manager}
              <button
                class="btn btn-sm text-gray/60 hover:variant-soft-warning"
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
