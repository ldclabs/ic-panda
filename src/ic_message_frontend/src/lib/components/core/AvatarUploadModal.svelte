<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { agent } from '$lib/stores/auth'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { imageUrl } from '$lib/utils/url'
  import ImageCrop from '$src/lib/components/ui/ImageCrop.svelte'
  import {
    BucketCanister,
    Uploader,
    toFixedChunkSizeReadable
  } from '@ldclabs/ic_oss_ts'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onDestroy, type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    myState: MyMessageState
  }

  let { parent, myState }: Props = $props()

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let blob: Blob | null = $state(null)
  let croppedUrl = $state('')
  let submitting = $state(false)

  function handleAvatarUpload(obj: { blob: Blob }) {
    if (obj.blob) {
      blob = obj.blob
      croppedUrl && URL.revokeObjectURL(croppedUrl)
      croppedUrl = URL.createObjectURL(obj.blob)
    }
  }

  function uploadImage() {
    if (submitting) return
    submitting = true
    toastRun(async () => {
      if (!blob) return
      const profileAPI = myState.agent.profileAPI
      const token = await profileAPI.upload_image_token({
        size: BigInt(blob.size),
        content_type: blob.type
      })
      const file = await toFixedChunkSizeReadable({
        content: blob,
        name: token.name,
        contentType: blob.type
      })
      const bucketClient = BucketCanister.create({
        agent,
        canisterId: token.image[0],
        accessToken: new Uint8Array(token.access_token)
      })
      const uploader = new Uploader(bucketClient)
      await uploader.upload_chunks(file, token.image[1], blob.size)
      const url = imageUrl(token.image[0], token.image[1], token.name)
      await myState.api.update_my_image(url)
      await myState.agent.setUser({
        ...myState.api.myInfo,
        image: url
      } as UserInfo)
      modalStore.close()
    }, toastStore)
  }

  onDestroy(() => {
    croppedUrl && URL.revokeObjectURL(croppedUrl)
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Update avatar</div>
  <div class="mx-auto !mt-6 space-y-4">
    <ImageCrop oncropcomplete={handleAvatarUpload} />

    <div class="mx-auto size-[200px] rounded bg-surface-500/10">
      {#if croppedUrl}
        <Avatar
          src={croppedUrl}
          border="border border-panda/90"
          background=""
          fill=""
          width="w-[200px]"
        />
      {/if}
    </div>
  </div>
  <button
    class="variant-filled-primary btn !mt-6 w-full"
    disabled={submitting || !blob}
    onclick={uploadImage}
    >{#if submitting}
      <span class=""><IconCircleSpin /></span>
      <span>Processing...</span>
    {:else}
      <span>Update</span>
    {/if}</button
  >
</ModalCard>
