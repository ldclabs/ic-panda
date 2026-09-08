import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  flushSync,
  mount,
  unmount,
  type ComponentProps,
  type SvelteComponent
} from 'svelte'
import { encodeCBOR } from '@ldclabs/cose-ts/utils'
import { Principal } from '@icp-sdk/core/principal'
import { setLocale } from '$lib/i18n'
import { getModalStore } from '$lib/ui/stores'
import { PANDAToken } from '$lib/utils/token'
import MemoDetail from './ui/MemoDetail.svelte'
import ImageCrop from './ui/ImageCrop.svelte'
import LinkEditModal from './messages/LinkEditModal.svelte'
import SendTokenForm from './ui/SendTokenForm.svelte'
import ModalHost from './ui/ModalHost.svelte'

vi.mock('svelte-easy-crop', async () => ({
  default: (await import('./testing/CropperStub.svelte')).default
}))

let target: HTMLDivElement
const dispose: (() => Promise<void>)[] = []
const parent = { onClose: vi.fn() } as unknown as SvelteComponent

beforeEach(async () => {
  await setLocale('en', false)
  getModalStore().clear()
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  for (const cleanup of dispose.splice(0).reverse()) await cleanup()
  getModalStore().clear()
  target.remove()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

function input(name: string): HTMLInputElement {
  const element = document.querySelector<HTMLInputElement>(
    `input[name="${name}"]`
  )
  if (!element) throw new Error(`Missing input: ${name}`)
  return element
}
function type(element: HTMLInputElement, value: string) {
  element.value = value
  element.dispatchEvent(new Event('input', { bubbles: true }))
  flushSync()
}

describe('prop and draft lifetimes', () => {
  it('updates decoded memo content when the prop changes', () => {
    const props = $state<ComponentProps<typeof MemoDetail>>({
      memo: encodeCBOR({ message: 'first memo', link: '' })
    })
    const component = mount(MemoDetail, { target, props })
    dispose.push(() => unmount(component))
    flushSync()
    expect(target.textContent).toContain('first memo')

    props['memo'] = encodeCBOR({
      message: 'second memo',
      link: 'https://example.com'
    })
    flushSync()
    expect(target.textContent).not.toContain('first memo')
    expect(target.textContent).toContain('second memo')
    expect(target.querySelector('a')?.href).toBe('https://example.com/')

    props['memo'] = null
    flushSync()
    expect(target.textContent?.trim()).toBe('')
  })

  it('preserves an edited draft when source metadata refreshes', () => {
    const props = $state<ComponentProps<typeof LinkEditModal>>({
      parent,
      link: { title: 'Original', uri: 'https://example.com/one', image: [] },
      onSave: vi.fn(async () => {})
    })
    const component = mount(LinkEditModal, { target, props })
    dispose.push(() => unmount(component))
    flushSync()
    type(input('titleInput'), 'Unsaved draft')

    props['link'] = {
      title: 'Server refresh',
      uri: 'https://example.com/two',
      image: []
    }
    flushSync()
    expect(input('titleInput').value).toBe('Unsaved draft')
    expect(input('uriInput').value).toBe('https://example.com/one')
  })

  it('initializes a new draft when a queued dialog uses the same component', () => {
    const store = getModalStore()
    const settings = (title: string) => ({
      type: 'component' as const,
      component: {
        ref: LinkEditModal,
        props: {
          link: { title, uri: 'https://example.com', image: [] },
          onSave: vi.fn(async () => {})
        }
      }
    })
    store.trigger(settings('First'))
    const component = mount(ModalHost, { target })
    dispose.push(() => unmount(component))
    flushSync()
    type(input('titleInput'), 'First draft')

    store.trigger(settings('Second'))
    flushSync()
    expect(input('titleInput').value).toBe('First draft')
    store.close()
    flushSync()
    expect(input('titleInput').value).toBe('Second')
  })

  it('keeps an exact Max amount across balance and locale updates', async () => {
    const balance = 9007199254740999n
    const onSubmit = vi.fn(async () => 123n)
    const props = $state<ComponentProps<typeof SendTokenForm>>({
      token: PANDAToken,
      availableBalance: balance,
      sendFrom: Principal.anonymous(),
      onSubmit
    })
    const component = mount(SendTokenForm, { target, props })
    dispose.push(() => unmount(component))
    flushSync()
    // Validate the canonical address without requiring a canister or wallet.
    type(input('sendTo'), '2vxsx-fae')
    expect(input('sendTo').validationMessage).toBe('')
    target.querySelector<HTMLAnchorElement>('a')!.click()
    flushSync()
    const entered = input('amount').value

    props['availableBalance'] = balance + 1000n
    await setLocale('zh', false)
    flushSync()
    expect(input('amount').value).toBe(entered)
    const buttons = [...target.querySelectorAll<HTMLButtonElement>('button')]
    buttons.at(-1)!.click()
    flushSync()
    const reviewButtons = [
      ...target.querySelectorAll<HTMLButtonElement>('button')
    ]
    reviewButtons.at(-1)!.click()
    await Promise.resolve()
    expect(onSubmit).toHaveBeenCalledWith({
      to: '2vxsx-fae',
      amount: balance - PANDAToken.fee
    })
  })
})

it('replaces image reads on file changes and ignores an obsolete result', () => {
  class Reader {
    static all: Reader[] = []
    result: string | null = null
    onload: (() => void) | null = null
    abort = vi.fn()
    readAsDataURL = vi.fn()
    constructor() {
      Reader.all.push(this)
    }
    finish(value: string) {
      this.result = value
      this.onload?.()
    }
  }
  vi.stubGlobal('FileReader', Reader)
  const firstFile = new File(['first'], 'first.png', { type: 'image/png' })
  const secondFile = new File(['second'], 'second.png', { type: 'image/png' })
  const props = $state<ComponentProps<typeof ImageCrop>>({
    file: firstFile,
    oncropcomplete: vi.fn()
  })
  const component = mount(ImageCrop, { target, props })
  dispose.push(() => unmount(component))
  flushSync()
  const first = Reader.all[0]!
  expect(first.readAsDataURL).toHaveBeenCalledWith(firstFile)

  props['file'] = secondFile
  flushSync()
  expect(first.abort).toHaveBeenCalledOnce()
  const second = Reader.all[1]!
  expect(second.readAsDataURL).toHaveBeenCalledWith(secondFile)
  first.finish('old image')
  flushSync()
  expect(target.querySelector('[data-crop-image]')).toBeNull()
  second.finish('new image')
  flushSync()
  expect(
    target.querySelector('[data-crop-image]')?.getAttribute('data-crop-image')
  ).toBe('new image')

  props['file'] = null
  flushSync()
  expect(second.abort).toHaveBeenCalledOnce()
  expect(target.querySelector('[data-crop-image]')).toBeNull()
})
