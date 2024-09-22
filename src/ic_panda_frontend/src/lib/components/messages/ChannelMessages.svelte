<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconMore2Line from '$lib/components/icons/IconMore2Line.svelte'
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
    elementsInViewport,
    isActive,
    scrollOnHooks
  } from '$lib/utils/window'
  import {
    getCurrentTimeString,
    toDisplayUserInfo,
    type ChannelInfoEx,
    type MessageInfo,
    type MyMessageState
  } from '$src/lib/stores/message'
  import { sleep } from '$src/lib/utils/helper'
  import { initPopup } from '$src/lib/utils/Popup'
  import { Avatar, getToastStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>
  export let channelInfo: ChannelInfoEx

  const MaybeMaxMessageId = 0xffff0000
  const toastStore = getToastStore()
  const { canister, id } = channelInfo

  type MessageInfoEx = MessageInfo & { pid?: number }

  let messageFeed: Writable<MessageInfoEx[]> = writable([])
  let latestMessage: Readable<MessageInfo | null>
  let elemChat: HTMLElement
  let dek: AesGcmKey
  let channelAPI: ChannelAPI
  let PendingMessageId = MaybeMaxMessageId

  // Messages
  let submitting = 0
  let newMessage = ''
  let messageStart = 1
  let latestMessageId = 1
  let lastRead = 1

  function sortMessages(msgs: MessageInfo[]): MessageInfo[] {
    msgs.sort((a, b) => a.id - b.id)
    return msgs
  }

  function addMessageInfos(infos: MessageInfoEx[]) {
    messageFeed.update((prev) => {
      if (infos.length === 0) {
        return prev
      }
      const rt: MessageInfoEx[] = [...prev]
      for (const info of infos) {
        let found = false
        const pid = info.pid || 0
        for (let i = 0; i < rt.length; i++) {
          if (pid > 0 && pid === rt[i]!.pid) {
            rt[i] = info
            found = true
            break
          } else if (info.id === rt[i]!.id) {
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
    if (!newMessage || submitting > 0) {
      return
    }

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
      submitting = PendingMessageId
      const msg: MessageInfoEx = {
        id: PendingMessageId,
        reply_to: 0,
        kind: 0,
        created_by: myState.principal,
        created_time: getCurrentTimeString(Date.now()),
        created_user: toDisplayUserInfo($myInfo),
        canister: canister,
        channel: id,
        message: newMessage,
        error: '',
        isDeleted: false,
        pid: PendingMessageId
      }
      newMessage = ''
      addMessageInfos([msg])
      await tick()
      scrollIntoView(msg.id, 'smooth')

      const res = await channelAPI.add_message(input)
      msg.id = res.id
      delete msg.pid
      msg.created_time = getCurrentTimeString(res.created_at)
      addMessageInfos([msg])

      submitting = 0
      await sleep(314)
    }, toastStore).finally(() => {
      submitting = 0
    })
  }

  function onPromptKeydown(event: KeyboardEvent): void {
    if (!event.shiftKey && !submitting && ['Enter'].includes(event.code)) {
      event.preventDefault()
      sendMessage()
    }
  }

  const debouncedUpdateMyLastRead = debounce(
    async () => {
      await myState.updateMyLastRead(canister, id, lastRead)
    },
    5000,
    { immediate: true }
  )

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
        latestMessageId + 1,
        dek
      )
    }
  }

  function updateLastRead(messageId: number) {
    if (messageId > lastRead) {
      lastRead = messageId
      myState.updateMyChannelSetting(canister, id, {
        last_read: lastRead
      })
      debouncedUpdateMyLastRead()
    }
  }

  const { popupState, popupOpenOn, popupDestroy } = initPopup({
    target: 'popupMessageOperation',
    triggerNodeClass: 'popup-trigger',
    placement: 'right-start'
  })

  function onPopupDeleteMessage() {
    const msg = { ...popupState.meta }
    if (msg) {
      submitting = msg.id
      myState
        .deleteMessage(msg.canister, msg.channel, msg.id)
        .then(() => {
          msg.payload = new Uint8Array()
          msg.message = ''
          msg.error = `Message is deleted by ${msg.created_user.name}`
          msg.isDeleted = true
          addMessageInfos([msg])
        })
        .finally(() => {
          submitting = 0
        })
    }
  }

  onMount(() => {
    const { abort } = toastRun(
      async (signal: AbortSignal, abortingQue: (() => void)[]) => {
        abortingQue.push(popupDestroy)

        channelInfo = await myState.refreshChannel(channelInfo)
        messageStart = channelInfo.message_start
        latestMessageId = channelInfo.latest_message_id
        lastRead = Math.min(channelInfo.my_setting.last_read, MaybeMaxMessageId)
        if (lastRead === 0) {
          lastRead = latestMessageId
        }

        channelAPI = await myState.api.channelAPI(canister)
        dek = await myState.decryptChannelDEK(channelInfo)
        await loadPrevMessages(messageStart, lastRead + 1)
        await tick()
        scrollIntoView(lastRead)

        await loadNextMessages(lastRead + 1)
        // no scroll
        if (elemChat?.scrollTop == 0) {
          const msg = $messageFeed.at(-1)
          if (msg && msg.id > lastRead && msg.id !== msg.pid) {
            lastRead = msg.id
            debouncedUpdateMyLastRead()
          }
        } else {
          scrollIntoView(lastRead + 1, 'smooth')
        }

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

            // messageId may be a pid
            updateLastRead(Math.min(messageId, latestMessageId))
          }
        })
        abortingQue.push(abortScroll)

        const polling = async () => {
          if (signal.aborted) return
          let msgs: any[] = []
          if (isActive() && latestMessage) {
            msgs = await myState.agent.fetchLatestMessages(
              canister,
              id,
              latestMessageId + 1
            )
          }

          setTimeout(polling, msgs.length > 0 ? 3000 : 7000)
        }
        polling()
      },
      toastStore
    )

    return abort
  })

  onDestroy(() => {
    debouncedUpdateMyLastRead.flush()
  })

  $: {
    const info = $latestMessage
    if (info && elemChat) {
      latestMessageId = info.id
      addMessageInfos([info])
      tick().then(() => {
        const msg = $messageFeed.at(-1)
        if (msg && msg.id > lastRead && msg.id !== msg.pid) {
          const ele = document.getElementById(
            `${canister.toText()}:${id}:${msg.id}`
          )
          if (ele && elementsInViewport(elemChat, [ele]).length > 0) {
            updateLastRead(msg.id)
          }
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
    <div class="card max-w-sm bg-white p-0" data-popup="popupMessageOperation">
      <div
        class="flex flex-col items-start divide-y divide-gray/5 p-2 text-gray"
      >
        <button class="btn btn-sm" on:click={onPopupDeleteMessage}>
          <span class="*:size-4"><IconDeleteBin /></span><span>Delete</span>
        </button>
      </div>
    </div>
    <div class="grid justify-center">
      <span
        class="text-panda/50 transition duration-300 ease-out {topLoading
          ? 'visible scale-125'
          : 'invisible scale-50'}"><Loading /></span
      >
    </div>
    {#each $messageFeed as msg (msg.id)}
      {#if msg.isDeleted}
        <div class="grid justify-center">
          <p class="text-balance bg-transparent p-2 text-xs text-gray/60"
            >{msg.error}</p
          >
        </div>
      {:else if msg.created_by.compareTo(myState.principal) !== 'eq'}
        <div
          class="grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <Avatar
            initials={msg.created_user.name}
            src={msg.created_user.image}
            fill="fill-white"
            width="w-10"
          />
          <div class="flex flex-col">
            <header
              class="flex items-center justify-between text-sm text-gray/60"
            >
              <p>{msg.created_user.name}</p>
              <small>{msg.created_time}</small>
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
                <p class="w-full text-balance px-4 py-2 text-sm text-gray/60"
                  >{msg.error}</p
                >
              {:else}
                <pre
                  class="w-full whitespace-break-spaces break-words px-4 py-2"
                  >{msg.message}</pre
                >
              {/if}
            </div>
          </div>
          <div></div>
        </div>
      {:else}
        <div
          class="group grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
          id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
        >
          <div class="mt-6 flex flex-row justify-end">
            {#if submitting === msg.id}
              <span class="pt-[10px] text-panda *:size-5"
                ><IconCircleSpin /></span
              >
            {:else}
              <button
                class="popup-trigger btn invisible h-10 p-0 group-hover:visible"
                on:click={(ev) => {
                  popupOpenOn(ev.currentTarget, msg)
                }}
              >
                <span class="text-gray/60 *:size-5"><IconMore2Line /></span>
              </button>
            {/if}
          </div>
          <div class=" flex flex-col">
            <header class="flex items-center justify-end text-sm text-gray/60">
              <small>{msg.created_time}</small>
            </header>
            <div
              class="card max-h-[600px] min-h-12 w-full overflow-auto overscroll-auto rounded-tr-none border-none {msg.kind ===
              1
                ? 'bg-transparent text-xs text-gray/60'
                : 'variant-soft-primary text-black'}"
            >
              {#if msg.error}
                <p class="w-full text-balance px-4 py-2 text-sm text-gray/60"
                  >{msg.error}</p
                >
              {:else}
                <pre
                  class="w-full whitespace-break-spaces break-words px-4 py-2"
                  >{msg.message}</pre
                >
              {/if}
            </div>
          </div>
          <Avatar
            initials={msg.created_user.name}
            src={msg.created_user.image}
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
      class="input-group input-group-divider grid-cols-[1fr_52px] bg-gray/5 rounded-container-token"
    >
      <TextArea
        bind:value={newMessage}
        onKeydown={onPromptKeydown}
        minHeight="40"
        maxHeight="200"
        containerClass=""
        class="textarea whitespace-break-spaces break-words border-0 bg-transparent outline-0 ring-0"
        name="prompt"
        id="prompt"
        disabled={submitting > 0}
        placeholder="Write a message..."
      />
      <button
        class="input-group-shim"
        disabled={submitting > 0 || !newMessage.trim()}
        on:click={sendMessage}
      >
        {#if submitting}
          <span class="text-gray/20 *:size-5"><IconCircleSpin /></span>
        {:else}
          <span
            class="transition duration-700 ease-in-out *:size-5 {submitting > 0
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
