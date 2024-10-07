<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconMore2Line from '$lib/components/icons/IconMore2Line.svelte'
  import IconSendPlaneFill from '$lib/components/icons/IconArrowUpLine2.svelte'
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
  } from '$lib/stores/message'
  import { sleep } from '$lib/utils/helper'
  import { initPopup } from '$lib/utils/popup'
  import { Avatar, getToastStore, getModalStore } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'
  import ProfileModel from './ProfileModel.svelte'
  import { type Principal } from '@dfinity/principal'

  export let myState: MyMessageState
  export let myInfo: Readable<UserInfo>
  export let channelInfo: ChannelInfoEx

  const MaybeMaxMessageId = 0xffff0000
  const toastStore = getToastStore()
  const modalStore = getModalStore()
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
  let hasKEK = true

  function sortMessages(msgs: MessageInfo[]): MessageInfo[] {
    msgs.sort((a, b) => a.id - b.id)
    return msgs
  }

  function addMessages(msgs: MessageInfoEx[]) {
    messageFeed.update((prev) => {
      if (msgs.length === 0) {
        return prev
      }
      const rt: MessageInfoEx[] = [...prev]
      for (const msg of msgs) {
        let found = false
        const pid = msg.pid || 0
        for (let i = 0; i < rt.length; i++) {
          if (pid > 0 && pid === rt[i]!.pid) {
            if (msg.id < msg.pid!) {
              delete msg.pid
            }

            rt[i] = msg
            found = true
            break
          } else if (msg.id === rt[i]!.id) {
            rt[i] = msg
            found = true
            break
          }
        }
        if (!found) {
          rt.push(msg)
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

  function onClickUser(userId: Principal) {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ProfileModel,
        props: {
          myState,
          userId
        }
      }
    })
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
      addMessages([msg])
      await tick()
      scrollIntoView(msg.id, 'smooth')

      const res = await channelAPI.add_message(input)
      msg.id = res.id
      msg.created_time = getCurrentTimeString(res.created_at)
      addMessages([msg])

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
      addMessages(prevMessages)
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
      Math.min(start + 20, latestMessageId + 1)
    )

    if (messages.length > 0) {
      addMessages(messages)
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
    triggerNodeClass: 'popup-trigger'
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
          addMessages([msg])
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

        if (
          !channelInfo._kek ||
          channelInfo._sync_at < Date.now() - 5 * 60 * 1000
        ) {
          channelInfo = await myState.refreshChannel(channelInfo)
        }
        hasKEK = !!channelInfo._kek
        messageStart = channelInfo.message_start
        latestMessageId = channelInfo.latest_message_id
        lastRead = Math.min(channelInfo.my_setting.last_read, MaybeMaxMessageId)
        if (lastRead === 0) {
          lastRead = latestMessageId
        }

        channelAPI = myState.api.channelAPI(canister)
        dek = await myState.decryptChannelDEK(channelInfo)
        await loadPrevMessages(messageStart, lastRead + 1)
        await tick()
        scrollIntoView(lastRead)

        await loadNextMessages(lastRead + 1)
        // no scroll
        if (elemChat?.scrollTop == 0) {
          const msg = $messageFeed.at(-1)
          if (
            msg &&
            msg.id > channelInfo.my_setting.last_read &&
            msg.id !== msg.pid
          ) {
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
      addMessages([info])
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
  class="grid h-[calc(100dvh-120px)] grid-rows-[1fr_auto] md:h-[calc(100dvh-60px)]"
>
  <!-- Conversation -->
  {#if !hasKEK}
    <div class="flex flex-col">
      <p class="p-2 text-center text-error-500">No encryption key found.</p>
      <p class="flex flex-row justify-center p-2 text-error-500">
        <span class="inline"
          >Please go to channel settings to request the encryption key.</span
        >
        <span><IconArrowRightUp /></span>
      </p>
    </div>
  {:else}
    <section
      bind:this={elemChat}
      class="text-surface-900-50-token snap-y snap-mandatory scroll-py-8 space-y-4 overflow-y-auto scroll-smooth p-2 pb-10 md:p-4"
    >
      <div
        class="card z-10 w-40 max-w-sm bg-white p-0"
        data-popup="popupMessageOperation"
      >
        <div
          class="divide-gray/5 flex flex-col items-start divide-y p-2 text-sm"
        >
          <button
            type="button"
            class="btn btn-sm w-full justify-start"
            on:click={onPopupDeleteMessage}
          >
            <span class="*:size-5"><IconDeleteBin /></span><span>Delete</span>
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
            <p class="text-balance bg-transparent p-2 text-xs">{msg.error}</p>
          </div>
        {:else if msg.created_by.compareTo(myState.principal) !== 'eq'}
          <div
            class="grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
            id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
          >
            <button
              class="btn p-0"
              disabled={msg.created_user.username === '_'}
              on:click={() => {
                onClickUser(msg.created_by)
              }}
            >
              <Avatar
                initials={msg.created_user.name}
                src={msg.created_user.image}
                fill="fill-white"
                width="w-10"
              />
            </button>

            <div class="flex flex-col">
              <header class="flex flex-row items-center gap-2 text-sm">
                <p>{msg.created_user.name}</p>
                <small>{msg.created_time}</small>
              </header>
              <div class="flex flex-col items-start">
                <div
                  class="card max-h-[600px] w-fit overflow-auto overscroll-auto rounded-tl-none border-none {msg.kind !==
                    1 && msg.id > lastRead
                    ? 'shadow-md shadow-gold'
                    : ''}  {msg.kind === 1
                    ? 'bg-transparent text-xs'
                    : 'bg-white'}"
                >
                  {#if msg.error}
                    <p class="w-full text-balance px-4 py-2 text-sm"
                      >{msg.error}</p
                    >
                  {:else}
                    <pre
                      class="icpanda-message w-full whitespace-break-spaces break-words px-4 py-2"
                      >{msg.message}</pre
                    >
                  {/if}
                </div>
              </div>
            </div>
            <div></div>
          </div>
        {:else}
          <div
            class="group grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
            id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
          >
            <div></div>
            <div class="flex flex-col">
              <header class="flex flex-col items-end text-sm">
                <small>{msg.created_time}</small>
              </header>
              <div class="flex flex-col items-end">
                <div
                  class="card relative overflow-visible rounded-tr-none border-none {msg.kind ===
                  1
                    ? 'bg-transparent text-xs'
                    : 'variant-soft-primary text-black'}"
                >
                  <div class="absolute -left-6 top-0">
                    {#if submitting === msg.id}
                      <span class="text-panda *:size-5"><IconCircleSpin /></span
                      >
                    {:else if msg.kind !== 1}
                      <button
                        class="popup-trigger btn invisible h-10 p-0 group-hover:visible"
                        on:click={(ev) => {
                          popupOpenOn(ev.currentTarget, msg)
                        }}
                      >
                        <span class="*:size-5"><IconMore2Line /></span>
                      </button>
                    {/if}
                  </div>
                  <div class="max-h-[600px] overflow-auto overscroll-auto">
                    {#if msg.error}
                      <p class="text-balance px-4 py-2 text-sm">{msg.error}</p>
                    {:else}
                      <pre
                        class="icpanda-message w-full whitespace-break-spaces break-words px-4 py-2"
                        >{msg.message}</pre
                      >
                    {/if}
                  </div>
                </div>
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
  {/if}
  <section
    class="group relative border-t border-surface-500/20 py-2 pl-2 pr-16 md:pr-24"
  >
    <TextArea
      bind:value={newMessage}
      onKeydown={onPromptKeydown}
      minHeight="40"
      maxHeight="200"
      containerClass=""
      class="textarea whitespace-break-spaces break-words border-0 !bg-transparent outline-0 ring-0"
      name="prompt"
      id="prompt"
      disabled={submitting > 0}
      placeholder="Write a message..."
    />
    <button
      class="btn btn-sm absolute bottom-3 right-4 overflow-hidden rounded-full border-2 border-white bg-surface-500/60 py-1 text-white shadow-xl backdrop-blur-md *:transition-all *:duration-500 before:absolute before:-z-10 before:aspect-square before:w-full before:translate-y-full before:rounded-full before:bg-primary-500 before:transition-all before:duration-500 group-hover:bg-surface-300/60 {newMessage.trim()
        ? ' before:translate-y-0 before:scale-150'
        : ''}"
      disabled={submitting > 0 || !newMessage.trim()}
      on:click={sendMessage}
    >
      {#if submitting}
        <span class="*:size-5"><IconCircleSpin /></span>
      {:else}
        <span class="*:size-5">
          <IconSendPlaneFill />
        </span>
      {/if}
    </button>
  </section>
</div>
