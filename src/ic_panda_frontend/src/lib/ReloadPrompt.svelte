<script lang="ts">
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { useRegisterSW } from 'virtual:pwa-register/svelte'

  const toastStore = getToastStore()

  const { needRefresh, updateServiceWorker } = useRegisterSW({
    onRegistered(r) {
      r &&
        setInterval(() => {
          if (!(!r.installing && navigator)) return
          if ('connection' in navigator && !navigator.onLine) return

          console.log('Checking for sw update')
          r.update()
        }, 20 * 60000)
    },
    onRegisterError(error) {
      console.log('SW registration error', error)
    }
  })

  function close(res: { id: string; status: 'queued' | 'closed' }) {
    if (res.status === 'closed') {
      needRefresh.set(false)
    }
  }

  $: {
    if ($needRefresh) {
      toastStore.trigger({
        autohide: false,
        classes: 'bg-black',
        message: 'New version available, click on "Reload" to update.',
        action: {
          label: 'Reload',
          response: () => updateServiceWorker(true)
        },
        callback: close
      })
    }
  }
</script>

<!-- placeholder for the reload prompt -->
<div class="hidden"></div>
