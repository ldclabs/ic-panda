import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { encode } from 'cborg'
import { setLocale } from '$lib/i18n'
import {
  ANDROID_CALLBACK,
  IOS_CALLBACK,
  b64,
  encodeError,
  encodeResult
} from '$lib/login/protocol'
import LoginPage from './+page.svelte'

const signing = vi.hoisted(() => ({
  requestDelegation: vi.fn(),
  closeChannel: vi.fn()
}))
vi.mock('@icp-sdk/signer', () => ({
  Signer: class {
    openChannel = async () => {}
    requestDelegation = signing.requestDelegation
    closeChannel = signing.closeChannel
  }
}))

const state = new Uint8Array(32).fill(1)
const delegation = { delegations: [], publicKey: 'test-key' }
const assign = vi.fn()
const replace = vi.fn()
let target: HTMLDivElement
let page: ReturnType<typeof mount>

function openLogin(next_url: string) {
  const request = {
    v: 1,
    state,
    session_pubkey: Uint8Array.from([
      0x30,
      0x2a,
      0x30,
      0x05,
      0x06,
      0x03,
      0x2b,
      0x65,
      0x70,
      0x03,
      0x21,
      0x00,
      ...new Uint8Array(32).fill(2)
    ]),
    ttl_ms: 60_000,
    derivation_origin: 'https://dmsg.net',
    next_url
  }
  vi.stubGlobal('location', {
    pathname: '/_/login',
    hash: '#' + b64(encode(request)),
    assign,
    replace
  })
  page = mount(LoginPage, { target })
  flushSync()
  const button = target.querySelector<HTMLButtonElement>('.utility-action')!
  button.click()
  return button
}

beforeEach(async () => {
  await setLocale('en', false)
  target = document.createElement('div')
  document.body.append(target)
  signing.requestDelegation.mockResolvedValue({ toJSON: () => delegation })
})

afterEach(async () => {
  await unmount(page)
  target.remove()
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it.each(['success', 'cancel'])(
  'hands the Android %s result back on a fresh tap',
  async (outcome) => {
    if (outcome === 'cancel')
      signing.requestDelegation.mockRejectedValueOnce({ code: 3001 })
    const button = openLogin(ANDROID_CALLBACK)
    await vi.waitFor(() =>
      expect(button).toHaveTextContent('Return to the dMsg app')
    )
    const fragment =
      outcome === 'success'
        ? encodeResult(state, JSON.stringify(delegation))
        : encodeError(state, 'user_cancelled')
    expect(assign).not.toHaveBeenCalled()
    expect(replace).not.toHaveBeenCalled()
    expect(target.innerHTML).not.toContain(fragment)
    button.click()
    button.click()
    expect(replace).toHaveBeenCalledExactlyOnceWith(
      ANDROID_CALLBACK + '#' + fragment
    )
    expect(signing.requestDelegation).toHaveBeenCalledTimes(1)
  }
)

it('discards an unclaimed Android result when leaving the login page', async () => {
  const button = openLogin(ANDROID_CALLBACK)
  await vi.waitFor(() =>
    expect(button).toHaveTextContent('Return to the dMsg app')
  )
  window.dispatchEvent(new Event('pagehide'))
  flushSync()
  expect(target.querySelector('.utility-action')).toBeNull()
  expect(replace).not.toHaveBeenCalled()
})

it('ignores an Android response that arrives after leaving the page', async () => {
  const pending = Promise.withResolvers<{ toJSON: () => typeof delegation }>()
  signing.requestDelegation.mockReturnValueOnce(pending.promise)
  openLogin(ANDROID_CALLBACK)
  await vi.waitFor(() =>
    expect(signing.requestDelegation).toHaveBeenCalledOnce()
  )
  window.dispatchEvent(new Event('pagehide'))
  pending.resolve({ toJSON: () => delegation })
  await vi.waitFor(() => expect(signing.closeChannel).toHaveBeenCalledOnce())
  flushSync()
  expect(target.querySelector('.utility-action')).toBeNull()
  expect(assign).not.toHaveBeenCalled()
  expect(replace).not.toHaveBeenCalled()
})

it.each([IOS_CALLBACK, 'http://127.0.0.1:2345/auth/callback'])(
  'automatically returns to %s',
  async (callback) => {
    openLogin(callback)
    await vi.waitFor(() =>
      expect(assign).toHaveBeenCalledExactlyOnceWith(
        callback + '#' + encodeResult(state, JSON.stringify(delegation))
      )
    )
  }
)
