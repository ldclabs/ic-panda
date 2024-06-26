/// <reference lib="webworker" />

import { clientsClaim } from 'workbox-core'
import {
  cleanupOutdatedCaches,
  precacheAndRoute,
  type PrecacheEntry
} from 'workbox-precaching'

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
