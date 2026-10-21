import { afterEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { setLocale } from '$lib/i18n'
import Layout from './+layout.svelte'

vi.mock('$app/environment', () => ({ browser: true }))

afterEach(() => {
  document.head.innerHTML = ''
})

it('updates existing social metadata and image when the language changes', async () => {
  const html = readFileSync(resolve('src/app.html'), 'utf8')
  document.head.innerHTML = html.match(/<meta\s[^>]*>/g)!.join('')
  const target = document.createElement('div')
  document.body.append(target)
  await setLocale('en', false)
  const component = mount(Layout, { target })
  try {
    for (const lang of ['zh', 'fr', 'ar', 'en']) {
      await setLocale(lang, false)
      flushSync()
      const descriptions = document.querySelectorAll('meta[name="description"]')
      expect(descriptions).toHaveLength(1)
      const description = descriptions[0]!.getAttribute('content')
      expect(
        document
          .querySelector('meta[property="og:description"]')
          ?.getAttribute('content')
      ).toBe(description)
      expect(
        document
          .querySelector('meta[name="twitter:description"]')
          ?.getAttribute('content')
      ).toBe(description)
      const image = `https://dmsg.net/_assets/dmsg/og-image${lang === 'en' ? '' : `-${lang}`}.png`
      expect(
        document
          .querySelector('meta[property="og:image"]')
          ?.getAttribute('content')
      ).toBe(image)
      expect(
        document
          .querySelector('meta[name="twitter:image"]')
          ?.getAttribute('content')
      ).toBe(image)
    }
  } finally {
    await unmount(component)
    target.remove()
  }
})
