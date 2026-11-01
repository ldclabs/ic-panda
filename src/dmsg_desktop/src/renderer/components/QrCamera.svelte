<script lang="ts">
  import { onMount } from 'svelte'
  import { bridge, code, errorText } from '../rpc'
  import { t } from '../i18n/index.svelte'
  import { buttonClass } from './ui'
  import Notice from './page/Notice.svelte'

  let { onImage, onCancel }: { onImage: (image: string) => Promise<void>; onCancel: () => void } = $props()
  let video: HTMLVideoElement
  let alive = true
  let stream: MediaStream | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let error = $state('')
  let loading = $state(true)

  function stop() {
    if (timer) clearTimeout(timer)
    stream?.getTracks().forEach(track => track.stop())
    stream = null
    if (video) video.srcObject = null
  }

  async function frame() {
    if (!alive || !stream) return
    if (video.readyState >= 2 && video.videoWidth) {
      const canvas = document.createElement('canvas')
      canvas.width = Math.min(1024, video.videoWidth)
      canvas.height = Math.round(video.videoHeight * canvas.width / video.videoWidth)
      const context = canvas.getContext('2d')!
      context.drawImage(video, 0, 0, canvas.width, canvas.height)
      try {
        await onImage(canvas.toDataURL('image/jpeg', 0.9).split(',')[1])
        stop()
        return
      } catch (e) {
        if (!alive) return
        if (code(e) !== 'NOT_FOUND') { error = errorText(e); stop(); return }
      }
    }
    if (alive && stream) timer = setTimeout(() => void frame(), 500)
  }

  async function start() {
    stop(); error = ''; loading = true
    try {
      if (!await bridge<boolean>(() => window.dmsg.cameraAccess())) throw new Error('camera unavailable')
      if (!alive) return
      const next = await navigator.mediaDevices.getUserMedia({ audio: false, video: { width: { ideal: 1280 }, height: { ideal: 720 }, facingMode: 'environment' } })
      if (!alive) { next.getTracks().forEach(track => track.stop()); return }
      stream = next
      video.srcObject = next
      await video.play()
      if (alive) void frame()
    } catch { if (alive) { error = t('qrCameraUnavailable'); stop() } }
    finally { if (alive) loading = false }
  }

  onMount(() => { void start(); return () => { alive = false; stop() } })
</script>

<div class="grid gap-3" data-testid="qr-camera">
  {#if error}<Notice tone="danger" testid="qr-camera-error">{error}</Notice>{/if}
  {#if loading}<p role="status" class="text-[12.5px] text-muted">{t('loading')}</p>{/if}
  <video bind:this={video} muted playsinline aria-label={t('scanQrCamera')} class="max-h-[min(240px,calc(100vh-300px))] w-full rounded-xl bg-paper-dim object-contain" data-testid="qr-camera-video"></video>
  <div class="flex flex-wrap gap-2">
    {#if error}<button class={buttonClass('secondary', 'sm')} onclick={start}>{t('retry')}</button>{/if}
    <button class={buttonClass('secondary', 'sm')} onclick={onCancel}>{t('cancel')}</button>
  </div>
</div>
