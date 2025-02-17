<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import { ChannelAPI } from '$lib/canisters/messagechannel'
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import IconArrowUpLine2 from '$lib/components/icons/IconArrowUpLine2.svelte'
  import IconCheckLine from '$lib/components/icons/IconCheckLine.svelte'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDeleteBin from '$lib/components/icons/IconDeleteBin.svelte'
  import IconEmotionHappyLine from '$lib/components/icons/IconEmotionHappyLine.svelte'
  import IconFileImageLine from '$lib/components/icons/IconFileImageLine.svelte'
  import IconMore2Line from '$lib/components/icons/IconMore2Line.svelte'
  import EmojisPopup from '$lib/components/ui/EmojisPopup.svelte'
  import Loading from '$lib/components/ui/Loading.svelte'
  import TextArea from '$lib/components/ui/TextAreaAutosize.svelte'
  import {
    toDisplayUserInfo,
    type ChannelInfoEx,
    type DisplayUserInfoEx,
    type MessageInfo,
    type MyMessageState
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import {
    MessageDetail,
    MessageKind,
    type FilePayload
  } from '$lib/types/message'
  import { dynAgent } from '$lib/utils/auth'
  import { coseA256GCMEncrypt0, type AesGcmKey } from '$lib/utils/crypto'
  import {
    getBytesString,
    getCurrentTimeString,
    sleep
  } from '$lib/utils/helper'
  import { initPopup } from '$lib/utils/popup'
  import {
    elementsInViewport,
    isActive,
    scrollOnHooks
  } from '$lib/utils/window'
  import { type Principal } from '@dfinity/principal'
  import {
    BucketCanister,
    Uploader,
    toFixedChunkSizeReadable,
    type Progress
  } from '@ldclabs/ic_oss_ts'
  import {
    Avatar,
    getModalStore,
    getToastStore,
    popup,
    type PopupSettings
  } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { onDestroy, onMount, tick } from 'svelte'
  import { writable, type Readable, type Writable } from 'svelte/store'
  import ChannelFileCard from './ChannelFileCard.svelte'
  import ChannelUploadModal from './ChannelUploadModal.svelte'
  import ProfileModal from './ProfileModal.svelte'

  interface Props {
    myState: MyMessageState
    myInfo: Readable<UserInfo>
    channelInfo: ChannelInfoEx
  }

  let { myState, myInfo, channelInfo = $bindable() }: Props = $props()

  const MaybeMaxMessageId = 0xffff0000
  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const { canister, id } = channelInfo
  const messageCacheKey = `${canister.toText()}:${id}:NewMessage`

  const emojisPopup: PopupSettings = {
    event: 'click',
    target: 'popupEmojisCard',
    placement: 'top',
    closeQuery: '' // important
  }

  type MessageInfoEx = MessageInfo & { pid?: number }

  let messageFeed: Writable<MessageInfoEx[]> = writable([])
  let latestMessage: Readable<MessageInfo | null> | undefined = $state()
  let elemChat: HTMLElement | undefined = $state()
  let dek: AesGcmKey | undefined = $state()
  let channelAPI: ChannelAPI
  let PendingMessageId = MaybeMaxMessageId

  // Messages
  let submitting = $state(0)
  let newMessage = $state(sessionStorage.getItem(messageCacheKey) || '')
  let messageStart = 1
  let latestMessageId = $state(1)
  let lastRead = $state(1)
  let hasKEK = $state(true)
  let fileInput: HTMLInputElement | undefined = $state()

  function onUploadButtonClick() {
    if (fileInput) fileInput.click()
  }

  let uploading: (Progress & { name: string }) | null = $state(null)
  let filePayload: FilePayload | null = $state(null)

  function onUploadChangeHandler(e: Event): void {
    const files = (e.target as HTMLInputElement)?.files || []
    if (files.length > 0) {
      onFilesChange(files as FileList)
    }
  }

  function onFilesChange(files: FileList): void {
    const file = files[0] || null
    if (!file || !dek) return

    modalStore.trigger({
      type: 'component',
      component: {
        ref: ChannelUploadModal,
        props: {
          channel: channelInfo,
          file,
          encryptBlob: async (blob: Blob) => {
            return coseA256GCMEncrypt0(
              dek!,
              new Uint8Array(await blob.arrayBuffer()),
              new Uint8Array()
            )
          },
          onReady: (data: Uint8Array, mime: string) => {
            uploading = {
              filled: 0,
              size: file.size,
              name: file.name,
              chunkIndex: 0,
              concurrency: 1
            }

            toastRun(async () => {
              const api = await myState.api.channelAPI(channelInfo.canister)
              const token = await api.upload_file_token({
                size: BigInt(data.byteLength),
                content_type: mime,
                channel: channelInfo.id
              })
              const stream = await toFixedChunkSizeReadable({
                content: data,
                name: token.name,
                contentType: mime
              })
              const bucketClient = BucketCanister.create({
                agent: dynAgent,
                canisterId: token.storage[0],
                accessToken: new Uint8Array(token.access_token)
              })
              const uploader = new Uploader(bucketClient)
              await uploader.upload_chunks(
                stream,
                token.id,
                data.byteLength,
                null,
                [],
                (progress: Progress) => {
                  uploading = { ...progress, name: file.name }
                }
              )

              filePayload = {
                canister: token.storage[0].toUint8Array(),
                id: token.id,
                name: file.name,
                size: file.size,
                type: file.type
              }
              // cache for sending
              await myState.agent.setUploadingFile(
                channelInfo.canister,
                channelInfo.id,
                $state.snapshot(filePayload)
              )
            }, toastStore).finally(() => {
              uploading = null
            })
          }
        }
      }
    })
  }

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
        ref: ProfileModal,
        props: {
          myState,
          userId
        }
      }
    })
  }

  function addEmoji(emoji: string) {
    if (emoji) {
      newMessage += emoji
    }
  }

  function sendMessage() {
    newMessage = newMessage.trim()
    if (!messageReady || !dek) {
      return
    }

    submitting = PendingMessageId
    const detail = filePayload
      ? new MessageDetail(newMessage, MessageKind.File, filePayload)
      : new MessageDetail(newMessage)

    toastRun(async () => {
      const input = {
        reply_to: [] as [] | [number],
        channel: id,
        payload: await coseA256GCMEncrypt0(
          dek!,
          detail.toBytes(),
          new Uint8Array()
        )
      }
      const msg: MessageInfoEx = {
        id: PendingMessageId,
        reply_to: 0,
        kind: 0,
        created_by: myState.principal,
        created_time: getCurrentTimeString(Date.now()),
        created_user: toDisplayUserInfo($myInfo),
        canister: canister,
        channel: id,
        message: detail.message,
        error: '',
        isDeleted: false,
        detail,
        pid: PendingMessageId
      }

      newMessage = ''
      uploading = null
      filePayload = null
      sessionStorage.removeItem(messageCacheKey)
      addMessages([msg])
      await tick()
      scrollIntoView(msg.id, 'smooth', 'end')

      const res = await channelAPI.add_message(input)
      // file send, clean cache
      await myState.agent.deleteUploadingFile(
        channelInfo.canister,
        channelInfo.id
      )

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
    // https://www.zhangxinxu.com/wordpress/2023/02/js-enter-submit-compositionupdate/
    if (
      !event.shiftKey &&
      !submitting &&
      (event.keyCode == 13 ||
        (!event.isComposing && ['Enter'].includes(event.code)))
    ) {
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

  let topLoading = $state(false)
  async function loadPrevMessages(start: number, end: number) {
    if (topLoading || !dek) {
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

  let bottomLoading = $state(false)
  async function loadNextMessages(start: number) {
    if (bottomLoading || !dek) {
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
          msg.detail = null
          msg.error = `Message is deleted by ${msg.created_user.name}`
          msg.isDeleted = true
          addMessages([msg])
        })
        .finally(() => {
          submitting = 0
        })
    }
  }

  // mention
  let showMentionList = $state(false)
  let mentionQuery = $state('')
  let mentionPosition = $state(0)
  let mentionBottom = $state(0)
  let mentionLeft = $state(0)
  let channelMembers: DisplayUserInfoEx[] = $state([])

  function onPromptInput(event: Event) {
    const textarea = event.target as HTMLTextAreaElement
    const text = textarea.value
    const cursorPos = textarea.selectionStart || 0

    // Check for @ symbol
    const lastAtPos = text.lastIndexOf('@', cursorPos - 1)
    if (lastAtPos >= 0) {
      const query = text.slice(lastAtPos + 1, cursorPos).trim()
      mentionQuery = query
      mentionPosition = lastAtPos
      showMentionList = true
      const rect = textarea.getBoundingClientRect()
      const lineHeight = parseInt(window.getComputedStyle(textarea).lineHeight)
      const linesBeforeCursor =
        text.substring(0, cursorPos).split('\n').length - 1

      // 计算位置
      const cursorTop = rect.top + linesBeforeCursor * lineHeight
      const cursorLeft = rect.left + textarea.selectionStart

      // 确保列表框在视窗内
      const listHeight = 192 // max-h-48 = 192px
      const windowHeight = window.innerHeight
      const offset = 8 // 添加一些间距

      mentionBottom = Math.min(
        windowHeight - cursorTop - offset, // 下方空间
        listHeight
      )
      mentionLeft = Math.min(
        cursorLeft,
        window.innerWidth - 256 // w-64 = 256px
      )
    } else {
      showMentionList = false
    }
  }

  function selectMember(member: DisplayUserInfoEx) {
    const text = newMessage
    const before = text.slice(0, mentionPosition)
    const after = text.slice(mentionPosition + mentionQuery.length + 1)
    newMessage = `${before}@${member.username} ${after}`
    showMentionList = false
    mentionQuery = ''
  }

  onMount(() => {
    const { abort } = toastRun(
      async (signal: AbortSignal, abortingQue: (() => void)[]) => {
        abortingQue.push(popupDestroy)

        if (!channelInfo._kek) {
          channelInfo = await myState.refreshChannel(channelInfo)
        } else if (channelInfo._sync_at < Date.now() - 60 * 1000) {
          myState.loadChannelInfo(canister, id).then((info) => {
            channelInfo = info
          })
        }

        hasKEK = !!channelInfo._kek
        if (!hasKEK) {
          return
        }

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
        await tick()

        if (!elemChat) {
          return
        }

        // try to load unsend file
        filePayload = await myState.agent.getUploadingFile(
          channelInfo.canister,
          channelInfo.id
        )
        // no scroll
        if (elemChat.scrollTop == 0) {
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

        myState
          .channelMembers(channelInfo, $myInfo)
          .then((res) => (channelMembers = res))

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
    newMessage = newMessage.trim()
    if (newMessage) {
      sessionStorage.setItem(messageCacheKey, newMessage)
    }
    debouncedUpdateMyLastRead.trigger()
  })

  $effect(() => {
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
          if (
            ele &&
            elemChat &&
            elementsInViewport(elemChat, [ele]).length > 0
          ) {
            updateLastRead(msg.id)
          }
        }
      })
    }
  })

  let messageReady = $derived(
    submitting == 0 &&
      uploading == null &&
      (!!newMessage.trim() || !!filePayload)
  )
</script>

{#snippet msgDetail(msg: MessageInfo)}
  {#if msg.error}
    <p class="w-full text-pretty px-4 py-2 text-sm">{msg.error}</p>
  {:else}
    <pre
      class="icpanda-message min-h-4 w-full text-pretty break-words px-4 py-2"
      >{msg.message}</pre
    >
  {/if}
  {#if msg.detail}
    {@const file = msg.detail.asFile()}
    {#if file && dek}
      <ChannelFileCard {myState} {file} {dek} {canister} {id} />
    {/if}
  {/if}
{/snippet}

<div
  class="grid h-[calc(100dvh-120px)] grid-rows-[1fr_auto] md:h-[calc(100dvh-60px)]"
>
  <!-- Conversation -->
  {#if !hasKEK}
    <div class="flex flex-col">
      <p class="p-2 text-center text-error-500">Access key not available.</p>
      <p class="flex flex-row justify-center p-2 text-error-500">
        <span class="inline">Go to channel settings to request access.</span>
        <span><IconArrowRightUp /></span>
      </p>
    </div>
  {:else}
    <section
      bind:this={elemChat}
      class="text-surface-900-50-token snap-y snap-mandatory scroll-py-8 space-y-4 overflow-y-auto scroll-smooth p-2 pb-10 md:p-4"
    >
      <div
        class="card bg-surface-50-900-token z-20 max-w-96"
        data-popup="popupEmojisCard"
      >
        <EmojisPopup {addEmoji} />
      </div>
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
            onclick={onPopupDeleteMessage}
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
            <p class="text-pretty bg-transparent p-2 text-xs">{msg.error}</p>
          </div>
        {:else if msg.created_by.compareTo(myState.principal) !== 'eq'}
          <div
            class="grid grid-cols-[40px_minmax(200px,_1fr)_40px] gap-2"
            id={`${msg.canister.toText()}:${msg.channel}:${msg.id}`}
          >
            <button
              class="btn h-fit p-0"
              disabled={msg.created_user.username === '_'}
              onclick={() => {
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
                  {@render msgDetail(msg)}
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
                        onclick={(ev) => {
                          popupOpenOn(ev.currentTarget, msg)
                        }}
                      >
                        <span class="*:size-5"><IconMore2Line /></span>
                      </button>
                    {/if}
                  </div>
                  <div class="max-h-[600px] overflow-auto overscroll-auto">
                    {@render msgDetail(msg)}
                  </div>
                </div>
              </div>
            </div>
            <Avatar
              initials={msg.created_user.name}
              src={msg.created_user.image}
              background={msg.created_user.image ? '' : 'bg-panda'}
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
    <div class="flex flex-row items-center gap-0 pl-1 text-surface-500">
      <button
        class="btn btn-sm px-2 hover:text-black dark:hover:text-white"
        disabled={submitting > 0}
        use:popup={emojisPopup}
      >
        <span class="*:size-5"><IconEmotionHappyLine /></span>
      </button>
      <div class="flex flex-row items-center">
        <!-- NOTE: Don't use `hidden` as it prevents `required` from operating -->
        <div class="h-0 w-0 overflow-hidden">
          <input
            type="file"
            bind:this={fileInput}
            oninput={onUploadChangeHandler}
          />
        </div>
        <button
          class="btn btn-sm px-2 hover:text-black dark:hover:text-white"
          disabled={submitting > 0 || uploading != null || filePayload != null}
          onclick={onUploadButtonClick}
        >
          <span class="*:size-5">
            <IconFileImageLine />
          </span>
        </button>

        {#if filePayload}
          <div class="btn btn-sm px-2 text-primary-500">
            <span class="max-w-52 truncate"
              >{`${filePayload.name}, ${getBytesString(filePayload.size)}`}</span
            >
            <span class="*:size-5">
              <IconCheckLine />
            </span>
          </div>
        {:else if uploading}
          <div class="btn btn-sm truncate px-2">
            <span class="max-w-52 text-pretty break-words"
              >{`${uploading.name}, ${getBytesString(uploading.filled)}/${getBytesString(uploading.size || uploading.filled)}`}</span
            >
            <span class="*:size-5">
              <IconCircleSpin />
            </span>
          </div>
        {/if}
      </div>
    </div>
    <TextArea
      bind:value={newMessage}
      onKeydown={onPromptKeydown}
      onInput={onPromptInput}
      {onFilesChange}
      minHeight="40"
      maxHeight="200"
      containerClass=""
      class="textarea text-pretty break-words border-0 !bg-transparent outline-0 ring-0"
      name="prompt"
      id="prompt"
      disabled={submitting > 0}
      placeholder="Write a message..."
    />
    <button
      class="btn btn-sm absolute bottom-3 right-4 overflow-hidden rounded-full border-2 border-white bg-surface-500/60 py-1 text-white shadow backdrop-blur-md *:transition-all *:duration-500 before:absolute before:-z-10 before:aspect-square before:w-full before:rounded-full before:bg-primary-500 before:transition-all before:duration-500 group-hover:bg-surface-300/60 {messageReady
        ? 'before:translate-y-0 before:scale-150'
        : 'before:translate-y-full'}"
      disabled={!messageReady}
      onclick={sendMessage}
    >
      {#if submitting}
        <span class="*:size-5"><IconCircleSpin /></span>
      {:else}
        <span class="*:size-5">
          <IconArrowUpLine2 />
        </span>
      {/if}
    </button>
    {#if showMentionList}
      <div
        class="bg-surface-50-900-token fixed z-50 max-h-48 w-64 overflow-y-auto rounded-lg shadow-lg"
        style="bottom: {mentionBottom}px; left: {mentionLeft}px"
      >
        {#each channelMembers.filter((m) => m.username
              .toLowerCase()
              .includes(mentionQuery.toLowerCase()) || m.name
              .toLowerCase()
              .includes(mentionQuery.toLowerCase())) as member}
          <button
            class="btn flex w-full min-w-0 cursor-pointer items-center justify-start rounded-none px-4 py-1 hover:bg-panda/10"
            onclick={() => selectMember(member)}
          >
            <Avatar
              initials={member.name}
              src={member.image}
              fill="fill-white"
              width="w-10"
              class="min-w-10"
            />
            <div class="min-w-0 flex-1 text-left">
              <p class="truncate">{member.name}</p>
              <p class="truncate text-sm text-surface-500">@{member.username}</p
              >
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </section>
</div>
