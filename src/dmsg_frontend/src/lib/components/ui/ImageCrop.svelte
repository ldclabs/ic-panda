<script lang="ts">
  import IconCameraLine from '$lib/components/icons/IconCameraLine.svelte'
  import FileButton from '$lib/components/ui/FileButton.svelte'
  import debounce from 'debounce'
  import { onDestroy } from 'svelte'
  import type { OnCropCompleteEvent } from 'svelte-easy-crop'
  import Cropper from 'svelte-easy-crop'

  interface CropArea {
    x: number
    y: number
    width: number
    height: number
  }

  interface Props {
    class?: string
    cropSize?: {
      width: number
      height: number
    }
    cropShape?: 'rect' | 'round'
    imageType?: string
    quality?: number
    file?: File | null
    oncropcomplete: (obj: { blob: Blob }) => void
  }

  let {
    class:
      selfClass = 'mx-auto w-[200px] h-[200px] rounded *:rounded bg-surface-500/20',
    cropSize = { width: 200, height: 200 },
    cropShape = 'round',
    imageType = 'image/webp',
    quality = 0.7,
    file = null,
    oncropcomplete
  }: Props = $props()

  let image: string = $state('')
  let crop = $state({ x: 0, y: 0 })
  let zoom = $state(1)
  let croppedAreaPixels: CropArea | undefined
  let reader: FileReader | null = null
  let imageRevision = 0

  $effect(() => {
    readImage(file)
  })

  function onFileSelected(e: Event) {
    const file = (e.target as HTMLInputElement)?.files![0] || null
    readImage(file)
  }

  function readImage(file: File | null) {
    // Replacing a file cancels any read/crop still belonging to the old image.
    imageRevision++
    debouncedCrop.clear()
    reader?.abort()
    reader = null
    image = ''
    croppedAreaPixels = undefined
    crop = { x: 0, y: 0 }
    zoom = 1
    if (!file) return

    const pending = new FileReader()
    reader = pending
    pending.onload = () => {
      if (reader === pending && typeof pending.result === 'string') {
        image = pending.result
      }
    }
    pending.readAsDataURL(file)
  }

  function convert() {
    if (!croppedAreaPixels) return
    const revision = imageRevision
    const area = { ...croppedAreaPixels }
    const { width, height } = cropSize

    const canvas = document.createElement('canvas')
    const ctx = canvas.getContext('2d') as CanvasRenderingContext2D
    const img = new Image()
    img.crossOrigin = 'anonymous'
    img.onload = () => {
      if (revision !== imageRevision) return
      canvas.width = width
      canvas.height = height
      ctx.drawImage(
        img,
        area.x,
        area.y,
        area.width,
        area.height,
        0,
        0,
        width,
        height
      )

      canvas.toBlob(
        (blob) => {
          // default to 'image/webp' or 'image/png'
          if (blob && revision === imageRevision) oncropcomplete({ blob })
        },
        imageType,
        quality
      )
    }
    img.src = image
  }

  const debouncedCrop = debounce(convert, 100)

  onDestroy(() => {
    imageRevision++
    reader?.abort()
    reader = null
    debouncedCrop.clear()
  })

  function onCropComplete(e: OnCropCompleteEvent) {
    croppedAreaPixels = e.pixels
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
      oncropcomplete={onCropComplete}
    />
  {:else}
    <FileButton
      name="files"
      accept="image/*"
      button="btn btn-icon w-full h-full *:size-8 *:text-surface-500 outline-0 ring-0"
      onchange={onFileSelected}><IconCameraLine /></FileButton
    >
  {/if}
</div>
