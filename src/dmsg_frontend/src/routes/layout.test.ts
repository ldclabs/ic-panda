import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { get } from 'svelte/store'
import { LOCALE_STORAGE_KEY, locale, setLocale } from '$lib/i18n'
import { mockStorage } from '$lib/i18n/test-storage'
import Layout from './+layout.svelte'
import { load } from './+layout'

vi.mock('$app/environment', () => ({ browser: true }))

let app: HTMLDivElement

const run = async () =>
  (await load({} as Parameters<typeof load>[0])) as { prerendered: boolean }

beforeEach(async () => {
  mockStorage()
  await setLocale('en', false)
  localStorage.setItem(LOCALE_STORAGE_KEY, 'zh')
  app = document.createElement('div')
  app.id = 'dmsg-app'
  document.body.append(app)
})

afterEach(() => {
  app.remove()
  vi.unstubAllGlobals()
})

it('hydrates prerendered English markup, then shows the reader locale', async () => {
  app.innerHTML = '<main><h1>Prerendered</h1></main><script></script>'
  expect(await run()).toEqual({ prerendered: true })
  // Svelte keeps server-rendered {@html} on hydration, so the locale must
  // still match the English build until the layout has mounted.
  expect(get(locale)).toBe('en')

  const target = document.createElement('div')
  document.body.append(target)
  const component = mount(Layout, {
    target,
    props: { data: { prerendered: true } }
  })
  try {
    flushSync()
    await vi.waitFor(() => expect(get(locale)).toBe('zh'))
  } finally {
    await unmount(component)
    target.remove()
  }
})

it('renders client-only and deep-linked routes in the reader locale', async () => {
  // A fallback shell or a cleared deep link holds no prerendered markup.
  app.innerHTML = '<script></script>'
  expect(await run()).toEqual({ prerendered: false })
  expect(get(locale)).toBe('zh')
})
