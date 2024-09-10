<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconSendPlaneFill from '$lib/components/icons/IconSendPlaneFill.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { toastRun } from '$lib/stores/toast'
  import {
    coseA256GCMEncrypt0,
    encodeCBOR,
    type AesGcmKey
  } from '$lib/utils/crypto'
  import {
    getCurrentTimestamp,
    toDisplayUserInfo,
    type ChannelInfoEx,
    type MessageInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import type { Principal } from '@dfinity/principal'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>
  export let channelInfo: Readable<ChannelInfoEx>

  const toastStore = getToastStore()
  const { canister, id } = $channelInfo

  let messageFeed: Writable<MessageInfo[]> = writable([])
  let latestMessage: Readable<MessageInfo | null>
  let elemChat: HTMLElement
  let dek: AesGcmKey
  let channelAPI: ChannelAPI

  // Messages
  let submitting = false
  let newMessage = ''
  let latestMessageId = 1

  function sortMessages(msgs: MessageInfo[]): MessageInfo[] {
    msgs.sort((a, b) => a.id - b.id)
    return msgs
  }

  function addMessageInfo(info: MessageInfo) {
    messageFeed.update((prev) => {
      for (let i = 0; i < prev.length; i++) {
        if (prev[i]!.id === info.id) {
          prev[i] = info
          return sortMessages([...prev])
        }
      }
      return sortMessages([...prev, info])
    })
  }

  function scrollChatBottom(behavior?: ScrollBehavior): void {
    elemChat.scrollTo({
      top: elemChat.scrollHeight,
      behavior
    } as ScrollToOptions)
  }

  function sendMessage() {
    newMessage = newMessage.trim()
    if (!newMessage) {
      return
    }

    submitting = true
    toastRun(async () => {
      const input = {
        reply_to: [] as [] | [number],
        channel: id,
        payload: await coseA256GCMEncrypt0(
          dek,
          encodeCBOR(newMessage),
          new Uint8Array()
        )
      }

      const res = await channelAPI.add_message(input)
      addMessageInfo({
        id: res.id,
        reply_to: 0,
        kind: res.kind,
        created_by: myState.principal,
        created_time: getCurrentTimestamp(res.created_at),
        created_user: toDisplayUserInfo($myInfo),
        canister: canister,
        channel: id,
        message: newMessage,
        error: ''
      } as MessageInfo)

      newMessage = ''
      submitting = false
      await tick()
      scrollChatBottom('smooth')
    }, toastStore).finally(() => {
      submitting = false
    })
  }

  function onPromptKeydown(event: KeyboardEvent): void {
    if (!event.shiftKey && !submitting && ['Enter'].includes(event.code)) {
      event.preventDefault()
      sendMessage()
    }
  }

  const debouncedUpdateMyLastRead = debounce(async () => {
    await myState.updateMyLastRead(canister, id, latestMessageId)
  }, 10000)

  async function loadMessages(canister: Principal, id: number) {
    channelAPI = await myState.api.channelAPI(canister)
    dek = await myState.decryptChannelDEK($channelInfo)
    latestMessage = await myState.loadLatestMessageStream(
      canister,
      id,
      dek,
      $channelInfo.latest_message_id
    )

    const prevMessages = await myState.loadPrevMessages(
      canister,
      id,
      dek,
      $channelInfo.latest_message_id
    )

    if (prevMessages.length > 0) {
      messageFeed.update((prev) => [...prevMessages, ...prev])
    }

    await tick()
    debouncedUpdateMyLastRead()
    debouncedUpdateMyLastRead.trigger()
    scrollChatBottom()
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      if (!signal.aborted) {
        await loadMessages(canister, id)
      } else {
        goto('/_/messages')
      }
    }, toastStore)

    return abort
  })

  onDestroy(() => {
    debouncedUpdateMyLastRead.flush()
  })

  $: {
    const info = $latestMessage
    if (info) {
      latestMessageId = info.id
      addMessageInfo(info)
      debouncedUpdateMyLastRead()
    }
  }
</script>

<div class="grid max-h-[calc(100dvh-140px)] grid-rows-[1fr_auto] bg-gray/5">
  <!-- Conversation -->
  <section bind:this={elemChat} class="space-y-4 overflow-y-auto p-4 pb-40">
    {#each $messageFeed as msg (msg.id)}
      {#if msg.created_by.compareTo(myState.principal) !== 'eq'}
        <div
          class="grid grid-cols-[auto_1fr] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <Avatar
            initials={msg.created_user.name}
            fill="fill-white"
            width="w-10"
          />
          <div class="mr-14 flex flex-col">
            <header class="flex items-center justify-between">
              <p class="font-bold">{msg.created_user.name}</p>
              <small class="opacity-50">{msg.created_time}</small>
            </header>
            <div
              class="card w-full rounded-tl-none {msg.kind === 1
                ? 'border-none bg-transparent text-xs text-gray/60'
                : 'bg-white'}"
            >
              {#if msg.error}
                <p
                  class="variant-filled-error max-h-[600px] max-w-[480px] overflow-auto text-pretty px-4 py-2 text-error-500"
                  >{msg.error}</p
                >
              {:else}
                <pre
                  class="max-h-[600px] w-full overflow-auto text-pretty px-4 py-2"
                  >{msg.message}</pre
                >
              {/if}
            </div>
          </div>
        </div>
      {:else}
        <div
          class="grid grid-cols-[1fr_auto] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <div class="ml-14 flex flex-col">
            <header class="flex items-center justify-end">
              <small class="opacity-50">{msg.created_time}</small>
            </header>
            <div
              class="card variant-soft-primary rounded-tr-none {msg.kind === 1
                ? 'border-none bg-transparent text-xs text-gray/60'
                : 'bg-white'}"
            >
              {#if msg.error}
                <p
                  class="variant-filled-error max-h-[600px] max-w-[480px] overflow-auto text-pretty px-4 py-2 text-error-500"
                  >{msg.error}</p
                >
              {:else}
                <pre
                  class="max-h-[600px] w-full max-w-[480px] overflow-auto text-pretty px-4 py-2"
                  >{msg.message}</pre
                >
              {/if}
            </div>
          </div>
          <Avatar
            initials={msg.created_user.name}
            background="bg-panda"
            fill="fill-white"
            width="w-10"
          />
        </div>
      {/if}
    {/each}
  </section>
  <!-- Prompt -->
  <section class="self-end border-t border-surface-500/30 bg-white p-4">
    <div
      class="input-group input-group-divider grid-cols-[auto_1fr_auto] bg-gray/5 rounded-container-token"
    >
      <button class="input-group-shim" disabled>
        <span class="*:size-5 *:text-gray/20"><IconAdd /></span>
      </button>
      <TextArea
        bind:value={newMessage}
        onKeydown={onPromptKeydown}
        minHeight="40"
        maxHeight="200"
        class="textarea max-w-[500px] text-pretty border-0 bg-transparent outline-0 ring-0"
        name="prompt"
        id="prompt"
        disabled={submitting}
        placeholder="Write a message..."
      />
      <button
        class="input-group-shim"
        disabled={submitting || !newMessage.trim()}
        on:click={sendMessage}
      >
        {#if submitting}
          <span class="text-panda *:size-5"><Loading /></span>
        {:else}
          <span
            class="transition duration-700 ease-in-out *:size-5 {submitting
              ? ''
              : 'hover:scale-125'}"
          >
            <IconSendPlaneFill fill={newMessage.trim() ? 'fill-panda' : ''} />
          </span>
        {/if}
      </button>
    </div>
  </section>
</div>
