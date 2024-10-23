<script lang="ts">
  import IconCameraLine from '$lib/components/icons/IconCameraLine.svelte'
  import { FileButton } from '@skeletonlabs/skeleton'
  import debounce from 'debounce'
  import { createEventDispatcher } from 'svelte'
  import Cropper from 'svelte-easy-crop'

  interface CropArea {
    x: number
    y: number
    width: number
    height: number
  }

  let selfClass: string =
    'mx-auto w-[200px] h-[200px] rounded *:rounded bg-surface-500/20'

  export { selfClass as class }
  export let cropSize: {
    width: number
    height: number
  } = { width: 200, height: 200 }
  export let cropShape: 'rect' | 'round' = 'round'
  export let imageType = 'image/webp'
  export let quality: number = 0.7
  export let file: File | null = null

  let image: string
  let crop = { x: 0, y: 0 }
  let zoom = 1
  let croppedAreaPixels: CropArea

  readImage(file)

  const dispatch = createEventDispatcher<{
    cropcomplete: { blob: Blob }
  }>()

  function onFileSelected(e: Event) {
    const file = (e.target as HTMLInputElement)?.files![0] || null
    readImage(file)
  }

  function readImage(file: File | null) {
    if (file) {
      const reader = new FileReader()
      reader.onload = (e) => {
        image = e.target!.result as string
      }
      reader.readAsDataURL(file)
    }
  }

  function convert() {
    if (!croppedAreaPixels) return

    const canvas = document.createElement('canvas')
    const ctx = canvas.getContext('2d') as CanvasRenderingContext2D
    const img = new Image()
    img.crossOrigin = 'anonymous'
    img.onload = () => {
      canvas.width = cropSize.width
      canvas.height = cropSize.height
      ctx.drawImage(
        img,
        croppedAreaPixels.x,
        croppedAreaPixels.y,
        croppedAreaPixels.width,
        croppedAreaPixels.height,
        0,
        0,
        cropSize.width,
        cropSize.height
      )

      canvas.toBlob(
        (blob) => {
          // default to 'image/webp' or 'image/png'
          blob && dispatch('cropcomplete', { blob })
        },
        imageType,
        quality
      )
    }
    img.src = image
  }

  const debouncedCrop = debounce(convert, 100)

  function onCropComplete(e: CustomEvent) {
    croppedAreaPixels = e.detail.pixels
    debouncedCrop()
  }
</script>

<div class="relative {selfClass}">
  {#if image}
    <Cropper
      {image}
      bind:crop
      bind:zoom
      aspect={1}
      minZoom={0.5}
      maxZoom={10}
      restrictPosition={false}
      {cropSize}
      {cropShape}
      on:cropcomplete={onCropComplete}
    />
  {:else}
    <FileButton
      name="files"
      accept="image/*"
      button="btn btn-icon w-full h-full *:size-8 *:text-surface-500 outline-0 ring-0"
      on:change={onFileSelected}><IconCameraLine /></FileButton
    >
  {/if}
</div>
