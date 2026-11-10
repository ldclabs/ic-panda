import { describe, expect, it } from 'vitest'
import en from '$lib/i18n/messages/en'
import { APP_STORE, installFor, MAC_APP, WINDOWS_APP } from './downloads'

const IPHONE = 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_4 like Mac OS X)'
const IPAD = 'Mozilla/5.0 (iPad; CPU OS 18_4 like Mac OS X)'
const IPAD_AS_MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605'
const MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/130.0'
const WINDOWS = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130.0'
const ANDROID = 'Mozilla/5.0 (Linux; Android 15; Pixel 9) Chrome/130.0'
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) Firefox/130.0'

describe('installFor', () => {
  it('offers the desktop installer that fits the computer', () => {
    expect(installFor(MAC, 0).href).toBe(MAC_APP)
    expect(installFor(WINDOWS, 0).href).toBe(WINDOWS_APP)
  })

  it('sends a device without its own installer to the full list', () => {
    for (const agent of [ANDROID, LINUX])
      expect(installFor(agent, 5)).toEqual({
        href: '/#release',
        label: 'Download dMsg'
      })
  })

  it('offers the published App Store app on iPhones and iPads', () => {
    for (const [agent, touch] of [
      [IPHONE, 5],
      [IPAD, 5],
      [IPAD_AS_MAC, 5]
    ] as const) {
      expect(installFor(agent, touch)).toEqual({
        href: APP_STORE,
        label: 'Download on the App Store'
      })
    }
  })

  it('names only labels the catalogs translate', () => {
    for (const agent of [IPHONE, MAC, WINDOWS, ANDROID])
      expect(en).toHaveProperty([installFor(agent, 0).label])
  })
})
