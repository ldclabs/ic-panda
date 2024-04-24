<script lang="ts">
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { errMessage } from '$lib/types/result'
  import { sleep } from '$lib/utils/helper'
  import decodeQR from '@paulmillr/qr/decode'
  import { getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { onDestroy, onMount, type SvelteComponent } from 'svelte'

  export let parent: SvelteComponent
  export let title = 'Scan QR Code'

  let videoPlayer: HTMLVideoElement
  let canvas: HTMLCanvasElement

  const modalStore = getModalStore()
  const toastStore = getToastStore()

  async function accessWebcam() {
    const opt: any = {
      audio: false,
      video: true
    }
    // To require the rear camera
    if (/Mobile|Android|XiaoMi/i.test(navigator.userAgent)) {
      opt.video = {
        facingMode: 'environment'
      }
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia(opt)
      videoPlayer.srcObject = stream
      videoPlayer.play()
    } catch (err) {
      toastStore.trigger({
        autohide: false,
        hideDismiss: false,
        background: 'variant-filled-error',
        message: errMessage(err)
      })
    }
  }

  function stopWebcam() {
    const stream = videoPlayer?.srcObject as MediaStream
    stream?.getTracks().forEach(function (track) {
      track.stop()
    })
  }

  let scan = false
  async function scanQR() {
    const context = canvas.getContext('2d', { willReadFrequently: true })!

    while (true) {
      if (!videoPlayer || !videoPlayer.videoWidth || !videoPlayer.videoHeight) {
        await sleep(200)
        continue
      }

      scan = true
      canvas.width = videoPlayer.videoWidth
      canvas.height = videoPlayer.videoHeight
      context.drawImage(
        videoPlayer,
        0,
        0,
        videoPlayer.videoWidth,
        videoPlayer.videoHeight
      )

      const data = context.getImageData(
        0,
        0,
        videoPlayer.videoWidth,
        videoPlayer.videoHeight
      )
      try {
        const code = decodeQR(data)
        if ($modalStore[0]?.response) {
          $modalStore[0].response(code)
        }
        modalStore.close()
        return
      } catch (err) {}

      await sleep(15)
    }
  }

  onMount(() => {
    canvas = document.createElement('canvas')
    accessWebcam()
  })

  onDestroy(stopWebcam)
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold">{title}</div>
  <div class="relative">
    <!-- svelte-ignore a11y-media-has-caption -->
    <video bind:this={videoPlayer} on:play={scanQR} />
    {#if scan}
      <section class="absolute left-0 top-0 h-full w-full"></section>
    {/if}
  </div>
</ModalCard>

<style>
  @keyframes scan {
    0% {
      transform: translateY(0%);
    }
    50% {
      transform: translateY(100%);
    }
    100% {
      transform: translateY(0%);
    }
  }
  video {
    -webkit-transform: scaleX(-1);
    transform: scaleX(-1);
  }
  section {
    animation: scan 3s linear infinite;
  }
  section::before {
    position: absolute;
    content: '';
    width: 100%;
    height: 3px;
    background-color: rgba(5, 215, 36, 0.9);
    box-shadow: 0 0 18px 4px rgba(0, 255, 38, 0.9);
  }
</style>
