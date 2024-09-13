<script lang="ts">
  import { goto } from '$app/navigation'
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconAdd from '$lib/components/icons/IconAdd.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconSendPlaneFill from '$lib/components/icons/IconSendPlaneFill.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import { toastRun } from '$lib/stores/toast'
  import {
    coseA256GCMEncrypt0,
    encodeCBOR,
    type AesGcmKey
  } from '$lib/utils/crypto'
  import { isActive, scrollOnHooks } from '$lib/utils/window'
  import {
    getCurrentTimeString,
    toDisplayUserInfo,
    type ChannelInfoEx,
    type MessageInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { sleep } from '$src/lib/utils/helper'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>
  export let channelInfo: ChannelInfoEx

  const toastStore = getToastStore()
  const { canister, id } = channelInfo

  let messageFeed: Writable<MessageInfo[]> = writable([])
  let latestMessage: Readable<MessageInfo | null>
  let elemChat: HTMLElement
  let dek: AesGcmKey
  let channelAPI: ChannelAPI

  // Messages
  let submitting = false
  let newMessage = ''
  let messageStart = 1
  let latestMessageId = 1
  let lastRead = 1

  function sortMessages(msgs: MessageInfo[]): MessageInfo[] {
    msgs.sort((a, b) => a.id - b.id)
    return msgs
  }

  function addMessageInfos(infos: MessageInfo[]) {
    messageFeed.update((prev) => {
      if (infos.length === 0) {
        return prev
      }
      const rt: MessageInfo[] = [...prev]
      for (const info of infos) {
        let found = false
        for (let i = 0; i < rt.length; i++) {
          if (rt[i]!.id === info.id) {
            rt[i] = info
            found = true
            break
          }
        }
        if (!found) {
          rt.push(info)
        }
      }
      return sortMessages(rt)
    })
  }

  function scrollIntoView(
    messageId: number,
    behavior: ScrollBehavior = 'instant',
    block: ScrollLogicalPosition = 'center'
  ): void {
    const ele = document.getElementById(
      `${canister.toText()}:${id}:${messageId}`
    )

    if (ele) {
      ele.scrollIntoView({
        block,
        behavior
      })
    }
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
      addMessageInfos([
        {
          id: res.id,
          reply_to: 0,
          kind: res.kind,
          created_by: myState.principal,
          created_time: getCurrentTimeString(res.created_at),
          created_user: toDisplayUserInfo($myInfo),
          canister: canister,
          channel: id,
          message: newMessage,
          error: ''
        } as MessageInfo
      ])

      newMessage = ''
      submitting = false
      await sleep(314)
      scrollIntoView(res.id, 'smooth')
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
    await myState.updateMyLastRead(canister, id, lastRead)
  }, 1000)

  let topLoading = false
  async function loadPrevMessages(start: number, end: number) {
    if (topLoading) {
      return
    }

    topLoading = true
    const prevMessages = await myState.loadMessages(
      canister,
      id,
      dek,
      start,
      end
    )
    if (prevMessages.length > 0) {
      addMessageInfos(prevMessages)
      await tick()
      scrollIntoView(prevMessages.at(-1)!.id, 'instant', 'start')
    }
    topLoading = false
  }

  let bottomLoading = false
  async function loadNextMessages(start: number) {
    if (bottomLoading) {
      return
    }

    bottomLoading = true
    const messages = await myState.loadMessages(
      canister,
      id,
      dek,
      start,
      start + 20
    )

    if (messages.length > 0) {
      addMessageInfos(messages)
    }

    bottomLoading = false
    await tick()
    const last = $messageFeed.at(-1)?.id || latestMessageId

    if (last >= latestMessageId && !$latestMessage) {
      latestMessageId = last
      latestMessage = await myState.loadLatestMessageStream(
        canister,
        id,
        dek,
        latestMessageId + 1,
        isActive
      )
    }
  }

  onMount(() => {
    const { abort } = toastRun(async (signal: AbortSignal) => {
      if (!signal.aborted) {
        if (!channelInfo._kek) {
          channelInfo = await myState.refreshMyChannel(channelInfo)
        }

        messageStart = channelInfo.message_start
        latestMessageId = channelInfo.latest_message_id
        lastRead = channelInfo.my_setting.last_read
        if (!lastRead) {
          lastRead = latestMessageId
        }
        channelAPI = await myState.api.channelAPI(canister)
        dek = await myState.decryptChannelDEK(channelInfo)
        await loadPrevMessages(messageStart, lastRead + 1)
        await tick()
        scrollIntoView(lastRead)

        await loadNextMessages(lastRead + 1)
        // no scroll
        if (elemChat.scrollTop == 0) {
          lastRead = $messageFeed.at(-1)!.id
          debouncedUpdateMyLastRead()
        }
      } else {
        goto('/_/messages')
      }
    }, toastStore)

    const abortScroll = scrollOnHooks(elemChat, {
      onTop: () => {
        if (dek && !topLoading) {
          const front = $messageFeed[0]
          if (front && front.id > messageStart) {
            loadPrevMessages(messageStart, front.id)
          }
        }
      },
      onBottom: () => {
        if (dek && !bottomLoading) {
          const back = $messageFeed.at(-1)
          if (back && back.id < latestMessageId) {
            loadNextMessages(back.id)
          }
        }
      },
      inMoveUpViewport: (els) => {
        const [_canister, _channel, mid] = els.at(-1)!.id.split(':')
        const messageId = parseInt(mid || '')

        if (messageId > lastRead) {
          lastRead = messageId
          myState.freshMyChannelSetting(canister, id, { last_read: messageId })
          myState.informMyChannelsStream()
          debouncedUpdateMyLastRead()
        }
      }
    })

    return () => {
      abortScroll()
      abort()
    }
  })

  onDestroy(() => {
    debouncedUpdateMyLastRead.flush()
  })

  $: {
    const info = $latestMessage
    if (info) {
      latestMessageId = info.id
      addMessageInfos([info])
      tick().then(() => {
        if (elemChat.scrollTop == 0) {
          lastRead = $messageFeed.at(-1)!.id
          debouncedUpdateMyLastRead()
        }
      })
    }
  }
</script>

<div
  class="grid h-[calc(100dvh-110px)] grid-rows-[1fr_auto] bg-gray/5 sm:h-[calc(100dvh-140px)]"
>
  <!-- Conversation -->
  <section
    bind:this={elemChat}
    class="snap-y snap-mandatory scroll-py-8 space-y-4 overflow-y-auto scroll-smooth p-2 pb-10 md:p-4"
  >
    <div class="grid justify-center">
      <span
        class="text-panda/50 transition duration-300 ease-out {topLoading
          ? 'visible scale-125'
          : 'invisible scale-50'}"><Loading /></span
      >
    </div>
    {#each $messageFeed as msg (msg.id)}
      {#if msg.created_by.compareTo(myState.principal) !== 'eq'}
        <div
          class="grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <Avatar
            initials={msg.created_user.name}
            fill="fill-white"
            width="w-10"
          />
          <div class="flex flex-col">
            <header class="flex items-center justify-between">
              <p class="font-bold">{msg.created_user.name}</p>
              <small class="opacity-50">{msg.created_time}</small>
            </header>
            <div
              class="card max-h-[600px] min-h-12 w-full overflow-auto overscroll-auto rounded-tl-none border-none {msg.kind !==
                1 && msg.id > lastRead
                ? 'shadow-md shadow-gold'
                : ''}  {msg.kind === 1
                ? 'bg-transparent text-xs text-gray/60'
                : 'bg-white'}"
            >
              {#if msg.error}
                <p
                  class="variant-filled-error text-pretty px-4 py-2 text-error-500"
                  >{msg.error}</p
                >
              {:else}
                <pre class="w-full text-pretty px-4 py-2">{msg.message}</pre>
              {/if}
            </div>
          </div>
          <div></div>
        </div>
      {:else}
        <div
          class="grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <div></div>
          <div class="flex flex-col">
            <header class="flex items-center justify-end">
              <small class="opacity-50">{msg.created_time}</small>
            </header>
            <div
              class="card max-h-[600px] min-h-12 w-full overflow-auto overscroll-auto rounded-tr-none border-none {msg.kind ===
              1
                ? 'bg-transparent text-xs text-gray/60'
                : 'bg-panda/20'}"
            >
              {#if msg.error}
                <p
                  class="variant-filled-error text-pretty px-4 py-2 text-error-500"
                  >{msg.error}</p
                >
              {:else}
                <pre class="w-full text-pretty px-4 py-2">{msg.message}</pre>
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
    <div class="grid justify-center">
      <span
        class="text-panda/50 transition duration-300 ease-out {bottomLoading
          ? 'visible scale-125'
          : 'invisible scale-0'}"><Loading /></span
      >
    </div>
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
          <span class="text-panda *:size-5"><IconCircleSpin /></span>
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
