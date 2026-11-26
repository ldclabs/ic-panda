import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { setLocale } from '$lib/i18n'
import DeviceDuo from './DeviceDuo.svelte'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined

beforeEach(async () => {
  vi.clearAllMocks()
  await setLocale('en', false)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    }
  )
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      matches: false,
      addEventListener() {},
      removeEventListener() {}
    }))
  )
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  target.remove()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('DeviceDuo terminal IME handling', () => {
  it('ignores Enter during IME composition and Safari keyCode 229, then runs normal Enter', () => {
    component = mount(DeviceDuo, { target })
    flushSync()

    const input = target.querySelector<HTMLInputElement>('.term-input input')!
    input.value = 'dmsg status'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    flushSync()

    input.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        isComposing: true,
        bubbles: true,
        cancelable: true
      })
    )
    flushSync()
    expect(input.value).toBe('dmsg status')

    const safariIme = new KeyboardEvent('keydown', {
      key: 'Enter',
      isComposing: false,
      bubbles: true,
      cancelable: true
    })
    Object.defineProperty(safariIme, 'keyCode', { value: 229 })
    input.dispatchEvent(safariIme)
    flushSync()
    expect(input.value).toBe('dmsg status')

    input.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        isComposing: false,
        bubbles: true,
        cancelable: true
      })
    )
    flushSync()
    expect(input.value).toBe('')
    expect(target.querySelector('.term-screen')?.textContent).toContain(
      'dmsg status'
    )
  })
})
