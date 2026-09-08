<script lang="ts">
  import { t } from '$lib/i18n'
  import { LEGACY_READ_ONLY } from '$lib/utils/legacy'
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import IconArrowLeftSLine from '$lib/components/icons/IconArrowLeftSLine.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import { type ChannelInfoEx, type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { sleep } from '$lib/utils/helper'
  import type { Principal } from '@icp-sdk/core/principal'
  import Avatar from '$lib/components/ui/Avatar.svelte'
  import { getToastStore } from '$lib/ui/stores'
  import { getContext, onMount, untrack } from 'svelte'
  import { type Readable } from 'svelte/store'
  import ChannelMessages from './ChannelMessages.svelte'
  import ChannelSetting from './ChannelSetting.svelte'

  interface Props {
    channelId: { canister: Principal; id: number }
    myState: MyMessageState
    myInfo: Readable<UserInfo>
  }

  let { channelId, myState, myInfo }: Props = $props()

  const toastStore = getToastStore()
  // Chat keys this component by channelParam. Keep the ID paired with the
  // onMount fetch until that instance is destroyed.
  const { canister, id } = untrack(() => channelId)
  const onChatBack = getContext('onChatBack') as () => void

  let channelInfo: ChannelInfoEx | null = $state(null)
  let isLoading = $state(true)
  let openMessages = $state(true)
  let switching = $state(false)
  async function onClickChannelSetting() {
    if (!canister) return
    switching = true
    await sleep(100)
    openMessages = !openMessages
    switching = false
  }

  onMount(() => {
    const { abort, finally: onfinally } = toastRun(
      async (signal: AbortSignal) => {
        if (canister) {
          channelInfo = await myState.loadChannelInfo(canister, id)
          openMessages = !!channelInfo._kek
          if (openMessages) {
            try {
              // try to decrypt channel DEK
              await myState.decryptChannelDEK(channelInfo)
            } catch (_e) {
              openMessages = false
            }
          }
          isLoading = false
          return true
        }
        return false
      },
      toastStore
    )

    onfinally((hasChannel) => {
      if (!hasChannel) {
        goto('/_/messages')
      }
    })

    return abort
  })
</script>

<div class="grid h-full min-h-0 grid-rows-[auto_1fr]">
  <header
    class="border-surface-500/20 flex h-[60px] flex-row items-center justify-between gap-2 border-b px-0 py-2 md:px-4"
  >
    <div class="md:hidden">
      <button
        aria-label={$t('Back to channels')}
        class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
        onclick={onChatBack}><IconArrowLeftSLine /></button
      >
    </div>
    <div class="flex flex-row items-center gap-2">
      {#if channelInfo}
        <Avatar
          initials={channelInfo.name}
          src={channelInfo.image}
          background={channelInfo.image ? '' : 'bg-panda'}
          fill="fill-white"
          width="w-10"
        />
        <span class="flex-1 text-start">
          {channelInfo.name +
            ' (' +
            (channelInfo.managers.length + channelInfo.members.length) +
            ')'}
        </span>
      {:else}
        <Avatar
          initials=""
          background="bg-panda"
          fill="fill-white"
          width="w-8"
        />
      {/if}
    </div>
    {#if !LEGACY_READ_ONLY}<button
        type="button"
        class="text-surface-900-50-token btn btn-icon hover:scale-125 hover:text-black dark:hover:text-white"
        title={$t('Channel settings')}
        disabled={switching}
        onclick={onClickChannelSetting}
      >
        {#if channelInfo && openMessages && (channelInfo?.ecdh_request || []).length > 0}
          <span class="badge-icon bg-error-500 z-10 size-2"></span>
        {/if}
        <span>
          {#if openMessages}
            <IconMoreFill />
          {:else}
            <IconClose />
          {/if}
        </span>
      </button>
    {/if}
  </header>
  {#if isLoading}
    <Loading />
  {:else if channelInfo}
    {#if openMessages}
      <ChannelMessages {myState} {myInfo} bind:channelInfo />
    {:else if LEGACY_READ_ONLY}
      <div class="archive-empty"
        ><h2>{$t('History could not be unlocked')}</h2><p
          >{$t(
            'This channel’s existing access key is missing or could not decrypt its content. Use your original browser and recovery materials. New key requests and key replacement are unavailable in the read-only app.'
          )}</p
        ></div
      >
    {:else}
      <ChannelSetting
        {myState}
        {myInfo}
        bind:channelInfo
        close={() => (openMessages = true)}
      />
    {/if}
  {:else}
    <div class="m-auto size-24 rounded-full *:size-24">
      <IconPanda />
    </div>
  {/if}
</div>
