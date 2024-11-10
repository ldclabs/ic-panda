<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import IconDownload from '$lib/components/icons/IconDownload.svelte'
  import ImagePreview from '$lib/components/ui/ImagePreview.svelte'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { type FilePayload } from '$lib/types/message'
  import {
    bytesToBase64Url,
    coseA256GCMDecrypt0,
    type AesGcmKey
  } from '$lib/utils/crypto'
  import { fetchFile } from '$lib/utils/fetcher'
  import { downloadUrl } from '$lib/utils/url'
  import type { Principal } from '@dfinity/principal'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onDestroy, onMount, tick } from 'svelte'

  interface Props {
    myState: MyMessageState
    file: FilePayload
    dek: AesGcmKey
    canister: Principal
    id: number
  }

  let { myState, file, dek, canister, id }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const aad = new Uint8Array()

  async function downloadFile(opts?: RequestInit) {
    const token = await myState.agent.getChannelStorageToken(canister, id)
    const downloadLink = downloadUrl(
      file.canister,
      file.id,
      file.name,
      bytesToBase64Url(token[0])
    )
    const blob = await fetchFile(downloadLink, opts)
    const buf = await blob.arrayBuffer()
    const data = new Uint8Array(buf)
    return await coseA256GCMDecrypt0(dek, data, aad)
  }

  let blobUrl: string
  let imageUrl = $state('')
  let downloading = $state(false)
  function onDownloadFile() {
    if (downloading) return
    downloading = true
    const { finally: onfinally } = toastRun(async (signal: AbortSignal) => {
      if (!blobUrl) {
        const data = await downloadFile({ signal })
        blobUrl = URL.createObjectURL(new Blob([data], { type: file.type }))
      }

      const a = document.createElement('a')
      a.download = file.name
      a.style.display = 'none'
      a.href = blobUrl
      document.body.appendChild(a)
      a.click()
      setTimeout(() => {
        document.body.removeChild(a)
        URL.revokeObjectURL(blobUrl)
      }, 100)
    }, toastStore)

    onfinally(() => (downloading = false))
  }

  function onPreviewImage() {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: ImagePreview,
        props: {
          imageUrl,
          imageName: file.name
        }
      }
    })
  }

  onMount(() => {
    const { abort } = toastRun(
      async (signal: AbortSignal, abortingQue: (() => void)[]) => {
        if (file.type.startsWith('image/')) {
          downloading = true
          await tick()
          const data = await downloadFile({ signal })
          blobUrl = URL.createObjectURL(new Blob([data], { type: file.type }))
          imageUrl = blobUrl
          downloading = false
          abortingQue.push(() => URL.revokeObjectURL(blobUrl))
        }
      },
      toastStore
    )

    return abort
  })

  onDestroy(() => {
    if (blobUrl) URL.revokeObjectURL(blobUrl)
  })
</script>

<div
  class="flex w-full flex-col items-center justify-center border-t border-surface-500/20"
>
  {#if imageUrl}
    <button
      type="button"
      class="w-full border-b border-surface-500/20 p-4"
      onclick={onPreviewImage}
    >
      <img src={imageUrl} alt={file.name} />
    </button>
  {:else if file.type.startsWith('image/') && downloading}
    <div class="flex h-10 w-full items-center justify-center *:size-5">
      <IconCircleSpin />
    </div>
  {/if}
  <div class="flex w-full flex-row items-center justify-center px-4">
    <p class="text-pretty break-all py-2"><span>{file.name}</span></p>
    <button
      type="button"
      class="btn btn-sm text-surface-500 hover:text-black dark:hover:text-white"
      disabled={downloading}
      onclick={onDownloadFile}
    >
      <span class="*:size-5">
        {#if downloading}
          <IconCircleSpin />
        {:else}
          <IconDownload />
        {/if}
      </span>
    </button>
  </div>
</div>
