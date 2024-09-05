<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI, type ChannelInfo } from '$lib/canisters/messagechannel'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconSendPlaneFill from '$lib/components/icons/IconSendPlaneFill.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { errMessage } from '$lib/types/result'
  import {
    coseA256GCMEncrypt0,
    encodeCBOR,
    type AesGcmKey
  } from '$lib/utils/crypto'
  import {
    getCurrentTimestamp,
    myMessageStateAsync,
    toDisplayUserInfo,
    type MessageInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import type { Principal } from '@dfinity/principal'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import { onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'

  export let channelId: string

  const toastStore = getToastStore()
  const { canister, id } = ChannelAPI.parseChannelParam(channelId)

  let myID: Principal
  let myState: MyMessageState
  let myInfo: Readable<UserInfo>
  let channelInfo: Readable<ChannelInfo>
  let messageFeed: Writable<MessageInfo[]> = writable([])
  let latestMessage: Readable<MessageInfo | null>
  let elemChat: HTMLElement
  let dek: AesGcmKey
  let channelAPI: ChannelAPI

  // Messages
  let submitting = false
  let newMessage = ''

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

  async function sendMessage() {
    newMessage = newMessage.trim()
    if (!newMessage) {
      return
    }

    try {
      submitting = true
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
        created_by: myID,
        created_time: getCurrentTimestamp(res.created_at),
        created_user: toDisplayUserInfo($myInfo),
        canister,
        message: newMessage,
        error: ''
      } as MessageInfo)

      newMessage = ''
      submitting = false
      await tick()
      scrollChatBottom('smooth')
    } catch (err: any) {
      submitting = false
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  function onPromptKeydown(event: KeyboardEvent): void {
    if (!event.shiftKey && !submitting && ['Enter'].includes(event.code)) {
      event.preventDefault()
      sendMessage()
    }
  }

  async function loadChannel(canister: Principal, id: number) {
    myState = await myMessageStateAsync()
    myInfo = myState.info
    myID = myState.principal
    channelInfo = await myState.loadChannelInfo(canister, id)
    channelAPI = await myState.api.channelAPI(canister)
    dek = await myState.decryptChannelDEK(
      canister,
      id,
      $channelInfo.dek as Uint8Array
    )

    latestMessage = await myState.loadLatestMessageStream(
      dek,
      canister,
      id,
      $channelInfo.latest_message_id
    )

    const prevMessages = await myState.loadPrevMessages(
      dek,
      canister,
      id,
      $channelInfo.latest_message_id
    )
    if (prevMessages.length > 0) {
      messageFeed.update((prev) => [...prevMessages, ...prev])
    }

    await tick()
    scrollChatBottom()
  }

  onMount(() => {
    if (canister) {
      loadChannel(canister, id).catch((err) =>
        console.error('loadChannel ERROR:', err)
      )
    } else {
      goto('/_/messages')
    }
  })

  $: {
    const info = $latestMessage
    if (info) {
      addMessageInfo(info)
    }
  }
</script>

<div
  class="grid-row-[1fr_minmax(80px,auto)] grid max-h-[calc(100dvh-76px)] rounded-tr-2xl bg-gray/5"
>
  <!-- Conversation -->
  <section bind:this={elemChat} class="space-y-4 overflow-y-auto p-4 pb-40">
    {#each $messageFeed as msg (msg.id)}
      {#if msg.created_by.compareTo(myID) !== 'eq'}
        <div class="grid grid-cols-[auto_1fr] gap-2">
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
            <div class="card w-full rounded-tl-none bg-white">
              {#if msg.error}
                <p
                  class="variant-filled-error max-h-[600px] max-w-[480px] overflow-auto text-pretty px-4 py-2 text-error-500"
                  >{msg.error}</p
                >
              {:else}
                <pre
                  class="max-h-[600px] max-w-[480px] overflow-auto text-pretty px-4 py-2"
                  >{msg.message}</pre
                >
              {/if}
            </div>
          </div>
        </div>
      {:else}
        <div class="grid grid-cols-[1fr_auto] gap-2">
          <div class="ml-14 flex flex-col">
            <header class="flex items-center justify-end">
              <small class="opacity-50">{msg.created_time}</small>
            </header>
            <div class="card variant-soft-primary rounded-tr-none">
              <pre
                class="max-h-[600px] max-w-[480px] overflow-auto text-pretty px-4 py-2"
                >{msg.message}</pre
              >
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
      <button class="input-group-shim"
        ><span class="*:size-5"><IconAdd /></span></button
      >
      <TextArea
        bind:value={newMessage}
        onKeydown={onPromptKeydown}
        minHeight="40"
        maxHeight="200"
        class="max-w-[500px] text-pretty border-0 bg-transparent outline-0 ring-0"
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
