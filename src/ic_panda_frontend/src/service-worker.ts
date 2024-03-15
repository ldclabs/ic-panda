/// <reference lib="webworker" />

import { clientsClaim, type WorkboxPlugin } from 'workbox-core'
import { ExpirationPlugin } from 'workbox-expiration'
import {
  cleanupOutdatedCaches,
  matchPrecache,
  precacheAndRoute,
  type PrecacheEntry
} from 'workbox-precaching'
import { registerRoute, setCatchHandler } from 'workbox-routing'
import {
  CacheFirst,
  NetworkFirst,
  StaleWhileRevalidate
} from 'workbox-strategies'

declare let self: ServiceWorkerGlobalScope

self.skipWaiting()
clientsClaim()

const precaches = self.__WB_MANIFEST
const indexHtml = precaches.find((entry) =>
  (entry as PrecacheEntry).url.endsWith('index.html')
) as PrecacheEntry | undefined

if (indexHtml) {
  precaches.push(Object.assign({}, indexHtml, { url: 'index.html' }))
}
precacheAndRoute(precaches)
cleanupOutdatedCaches()

registerRoute(
  new RegExp('https://panda\\.fans.*'),
  new StaleWhileRevalidate({
    cacheName: 'assets-res',
    plugins: [
      new ExpirationPlugin({
        maxEntries: 1000,
        maxAgeSeconds: 7 * 24 * 60 * 60
      }) as WorkboxPlugin
    ],
    matchOptions: {
      ignoreMethod: false,
      ignoreSearch: true,
      ignoreVary: false
    }
  })
)

registerRoute(
  new RegExp('https://icp-api\\.io/api/.*'),
  new NetworkFirst({
    cacheName: 'api-res',
    plugins: [
      new ExpirationPlugin({
        maxEntries: 10000,
        maxAgeSeconds: 3 * 24 * 60 * 60
      }) as WorkboxPlugin
    ],
    matchOptions: {
      ignoreMethod: false,
      ignoreSearch: false,
      ignoreVary: false
    }
  })
)

setCatchHandler(async ({ request }) => {
  let res: Response | undefined
  switch (request.destination) {
    case 'document':
      res = await matchPrecache('index.html')
    // case 'image':
    //   return matchPrecache(FALLBACK_IMAGE_URL)
  }

  return res || Response.error()
})
