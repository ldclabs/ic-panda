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
  import { onDestroy, onMount } from 'svelte'

  export let myState: MyMessageState
  export let file: FilePayload
  export let dek: AesGcmKey
  export let canister: Principal
  export let id: number

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
  let imageUrl = ''
  let downloading = false
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
          const data = await downloadFile({ signal })
          blobUrl = URL.createObjectURL(new Blob([data], { type: file.type }))
          imageUrl = blobUrl

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

<div class="flex w-full flex-col items-center justify-center">
  {#if imageUrl}
    <button
      type="button"
      class="w-full border-t border-surface-500/20 p-4"
      on:click={onPreviewImage}
    >
      <img src={imageUrl} alt={file.name} />
    </button>
  {/if}
  <button
    type="button"
    class="flex w-full flex-row items-center justify-center gap-2 text-pretty break-all rounded-none border-t border-surface-500/20 p-4"
    disabled={downloading}
    on:click={onDownloadFile}
  >
    <span>{file.name}</span>
    <span class="text-surface-500 *:size-5">
      {#if downloading}
        <IconCircleSpin />
      {:else}
        <IconDownload />
      {/if}
    </span>
  </button>
</div>
