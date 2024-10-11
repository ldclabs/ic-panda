<script lang="ts">
  import { type UserInfo } from '$lib/canisters/message'
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { agent } from '$lib/stores/auth'
  import { type MyMessageState } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { avatarUrl } from '$lib/utils/url'
  import ImageCrop from '$src/lib/components/ui/ImageCrop.svelte'
  import {
    BucketCanister,
    Uploader,
    toFixedChunkSizeReadable
  } from '@ldclabs/ic_oss_ts'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState

  const toastStore = getToastStore()
  const modalStore = getModalStore()

  let blob: Blob | null = null
  let croppedUrl = ''
  let submitting = false

  function handleAvatarUpload(event: CustomEvent) {
    blob = event.detail.blob || null
    if (blob) {
      croppedUrl = URL.createObjectURL(blob)
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
      const url = avatarUrl(token.image[0], token.image[1], token.name)
      await myState.api.update_my_image(url)
      await myState.agent.setUser({
        ...myState.api.myInfo,
        image: url
      } as UserInfo)
      modalStore.close()
    }, toastStore)
  }
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">Update avatar</div>
  <div class="mx-auto !mt-6 space-y-4">
    <ImageCrop on:cropcomplete={handleAvatarUpload} />

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
    on:click={uploadImage}
    >{#if submitting}
      <span class=""><IconCircleSpin /></span>
      <span>Processing...</span>
    {:else}
      <span>Update</span>
    {/if}</button
  >
</ModalCard>
