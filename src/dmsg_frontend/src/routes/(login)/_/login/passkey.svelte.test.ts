import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import { encode } from 'cborg'
import { setLocale } from '$lib/i18n'
import { completePasskey, type PasskeyRequest } from '$lib/login/passkey'
import { b64 } from '$lib/login/protocol'
import LoginPage from './+page.svelte'

vi.mock('$lib/login/passkey', async (original) => ({
  ...(await original<typeof import('$lib/login/passkey')>()),
  completePasskey: vi.fn().mockResolvedValue(undefined)
}))

const originalCredentials = Object.getOwnPropertyDescriptor(
  navigator,
  'credentials'
)
let target: HTMLDivElement
let component: ReturnType<typeof mount>
let request: PasskeyRequest

beforeEach(async () => {
  await setLocale('en', false)
  vi.stubGlobal('isSecureContext', true)
  vi.stubGlobal('PublicKeyCredential', class {})
  Object.defineProperty(navigator, 'credentials', {
    configurable: true,
    value: { create: vi.fn(), get: vi.fn() }
  })
  request = {
    protocol: 'dmsg-passkey/1',
    state: new Uint8Array(32).fill(1),
    challenge: {
      challenge: new Uint8Array(32).fill(2),
      expires_at: Date.now() + 300_000,
      action: 'register'
    },
    next_url: 'http://127.0.0.1:2345/auth/callback',
    credential_id: null
  }
  history.replaceState(null, '', `/_/login#${b64(encode(request))}`)
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  await unmount(component)
  target.remove()
  history.replaceState(null, '', '/')
  if (originalCredentials)
    Object.defineProperty(navigator, 'credentials', originalCredentials)
  else Reflect.deleteProperty(navigator, 'credentials')
  vi.unstubAllGlobals()
  vi.clearAllMocks()
})

it('offers a localized explanation and a return action without WebAuthn', () => {
  vi.stubGlobal('PublicKeyCredential', undefined)
  component = mount(LoginPage, { target })
  flushSync()
  expect(target.querySelector('[role="alert"]')).toHaveTextContent(
    'Passkeys are unavailable in this browser.'
  )
  expect(target.querySelector('.utility-note')).toBeNull()
  const button = target.querySelector<HTMLButtonElement>('.utility-action')!
  expect(button).toHaveTextContent('Return to the dMsg app')
  expect(button).toBeEnabled()
  button.click()
  expect(completePasskey).toHaveBeenCalledWith(
    request,
    navigator.credentials,
    expect.any(Function)
  )
})

it('offers registration without checking for a platform authenticator', () => {
  const platformAvailable = vi.fn().mockResolvedValue(false)
  vi.stubGlobal('PublicKeyCredential', {
    isUserVerifyingPlatformAuthenticatorAvailable: platformAvailable
  })
  component = mount(LoginPage, { target })
  flushSync()
  expect(target.querySelector('[role="alert"]')).toBeNull()
  const button = target.querySelector<HTMLButtonElement>('.utility-action')!
  expect(button).toHaveTextContent('Create a passkey')
  expect(button).toBeEnabled()
  expect(platformAvailable).not.toHaveBeenCalled()
  button.click()
  expect(completePasskey).toHaveBeenCalledWith(
    request,
    navigator.credentials,
    expect.any(Function)
  )
})
