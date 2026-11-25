import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { setLocale } from '$lib/i18n'
import CopyReceipt from './CopyReceipt.svelte'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined

beforeEach(async () => {
  vi.clearAllMocks()
  await setLocale('en', false)
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  target.remove()
  vi.restoreAllMocks()
})

describe('CopyReceipt', () => {
  it('re-announces repeated copies with an alternating non-breaking space', async () => {
    const writeText = vi.fn(async () => {})
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText }
    })

    component = mount(CopyReceipt, {
      target,
      props: { text: 'receipt-body', describedBy: 'file-1' }
    })
    flushSync()

    const button = target.querySelector<HTMLButtonElement>('button')!
    const status = target.querySelector<HTMLElement>('[role="status"]')!

    button.click()
    await vi.waitFor(() => {
      flushSync()
      expect(status.textContent).toBe('Copied')
    })

    button.click()
    await vi.waitFor(() => {
      flushSync()
      expect(status.textContent).toBe('Copied\u00A0')
    })
    expect(writeText).toHaveBeenCalledTimes(2)
    expect(target.querySelector('textarea')).toBeNull()
  })

  it('shows a selectable readonly fallback textarea when clipboard access fails', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn(async () => {
          throw new DOMException('Denied', 'NotAllowedError')
        })
      }
    })

    component = mount(CopyReceipt, {
      target,
      props: { text: 'file.txt\nSHA-256 abc', describedBy: 'file-2' }
    })
    flushSync()

    target.querySelector<HTMLButtonElement>('button')!.click()

    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('[role="status"]')?.textContent).toBe(
        'Could not copy'
      )
    })

    const textarea = target.querySelector<HTMLTextAreaElement>('textarea')!
    expect(textarea).not.toBeNull()
    expect(textarea.readOnly).toBe(true)
    expect(textarea.value).toBe('file.txt\nSHA-256 abc')

    const select = vi.spyOn(textarea, 'select')
    textarea.dispatchEvent(new FocusEvent('focus'))
    expect(select).toHaveBeenCalledOnce()
  })
})
