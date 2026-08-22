/**
 * Tombstone service worker — TEMPORARY, safe to delete after ~2027-03.
 *
 * panda.fans shipped a Workbox precaching service worker until the v2
 * refactor removed the PWA. Browsers that already registered the old worker
 * keep it alive and keep serving its precached app shell, so those visitors
 * would never see the new site.
 *
 * A registration only goes away when the script at its own URL changes, so
 * this file has to keep living at /service-worker.js. It claims the
 * registration, drops every cache the old worker created, unregisters itself
 * and reloads open tabs onto the network. It installs no fetch handler, so it
 * never serves anything from cache.
 *
 * Once enough time has passed that no meaningful number of clients still hold
 * the old registration, delete this file.
 */

self.addEventListener('install', () => {
  self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys()
      await Promise.all(keys.map((key) => caches.delete(key)))

      await self.registration.unregister()

      const clients = await self.clients.matchAll({ type: 'window' })
      for (const client of clients) {
        client.navigate(client.url)
      }
    })()
  )
})
