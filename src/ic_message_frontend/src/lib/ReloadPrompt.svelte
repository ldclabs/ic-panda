<script lang="ts">
  import { getToastStore } from '@skeletonlabs/skeleton'
  import { useRegisterSW } from 'virtual:pwa-register/svelte'

  const toastStore = getToastStore()
  const period = 60 * 60 * 1000

  function registerPeriodicSync(swUrl: string, r: ServiceWorkerRegistration) {
    console.log('Register periodic sync', swUrl)

    async function check() {
      if ('onLine' in navigator && navigator.onLine) {
        await r.update()
      }
      setTimeout(check, period)
    }

    setTimeout(check, 60 * 1000)
  }

  const { offlineReady, needRefresh, updateServiceWorker } = useRegisterSW({
    onRegisteredSW(swUrl, r) {
      if (r?.active?.state === 'activated') {
        registerPeriodicSync(swUrl, r)
      } else if (r?.installing) {
        r.installing.addEventListener('statechange', (e) => {
          const sw = e.target as ServiceWorker
          if (sw.state === 'activated') registerPeriodicSync(swUrl, r)
        })
      }
    },
    onRegisterError(error) {
      console.error('SW registration error', error)
    }
  })

  function close() {
    offlineReady.set(false)
    needRefresh.set(false)
  }

  $effect(() => {
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
  })
</script>

<!-- placeholder for the reload prompt -->
<div class="hidden"></div>
