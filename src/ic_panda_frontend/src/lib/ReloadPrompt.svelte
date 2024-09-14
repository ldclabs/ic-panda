<script lang="ts">
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { useRegisterSW } from 'virtual:pwa-register/svelte'

  const toastStore = getToastStore()
  const minInterval = 20 * 1000
  const maxInterval = 20 * 60 * 1000

  const { needRefresh, updateServiceWorker } = useRegisterSW({
    onRegistered(r) {
      if (r) {
        let i = 0
        const check = () => {
          i += 1
          setTimeout(check, i > 7 ? maxInterval : minInterval)

          if (!navigator.onLine) return
          console.log('Checking for sw update')
          r.update()
        }
        setTimeout(check, minInterval)
      }
    },
    onRegisterError(error) {
      console.error('SW registration error', error)
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
