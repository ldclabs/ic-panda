import {
  ENV,
  X_AUTH_CHALLENGE_ENDPIONT,
  X_AUTH_ENDPIONT,
  X_AUTH_KEY
} from '$lib/constants'
import { authStore, type AuthSignInParams } from '$lib/stores/auth'
import { type AuthMessage } from '$lib/types/auth'
import { createRequest, type SuccessResponse } from '$lib/utils/fetcher'
import { popupCenter } from '$lib/utils/window'
import type { Principal } from '@dfinity/principal'

export const signIn = async (
  params: AuthSignInParams
): Promise<{ success: 'ok' | 'cancelled' | 'error'; err?: unknown }> => {
  try {
    await authStore.signIn(params)

    return { success: 'ok' }
  } catch (err: unknown) {
    if (err === 'UserInterrupt') {
      // We do not display an error if user explicitly cancelled the process of sign-in
      return { success: 'cancelled' }
    }

    return { success: 'error', err }
  } finally {
  }
}

export const signOut = (): Promise<void> => logout({})

const logout = async ({
  msg = undefined,
  clearIdbAddress = true
}: {
  msg?: string
  clearIdbAddress?: boolean
}) => {
  await authStore.signOut()

  // Auth: Delegation and identity are cleared from indexedDB by agent-js so, we do not need to clear these

  // Preferences: We do not clear local storage as well. It contains anonymous information such as the selected theme.
  // Information the user want to preserve across sign-in. e.g. if I select the light theme, logout and sign-in again, I am happy if the dapp still uses the light theme.

  // We reload the page to make sure all the states are cleared
  window.location.reload()
}

export class XAuth {
  private url: URL
  protected constructor(
    principal: Principal,
    env: string,
    private _authWindow: Window | null = null,
    private _timeoutId: any = null,
    private _onSuccess: (result: string) => void = () => {},
    private _onFailure: (error: string) => void = () => {}
  ) {
    this.url = new URL(
      `${X_AUTH_ENDPIONT}?principal=${principal.toText()}&env=${env}`
    )
  }

  static async authorize(principal: Principal): Promise<string> {
    const xauth = new XAuth(principal, ENV)
    return new Promise((resolve, reject) => {
      localStorage.removeItem(X_AUTH_KEY)

      xauth._onSuccess = resolve
      xauth._onFailure = reject
      // xauth._eventHandler = xauth._onEvent.bind(xauth)
      // window.addEventListener('message', xauth._eventHandler)
      xauth._authWindow = window.open(
        xauth.url.toString(),
        'XAuthWindow',
        popupCenter({
          width: 576,
          height: 625
        })
      )
      let i = 0
      const checkInterruption = (): void => {
        if (i > 360) {
          xauth._onFailure('XAuth Timeout')
        } else if (!xauth._checkResult()) {
          i += 1
          xauth._timeoutId = setTimeout(checkInterruption, 300)
        }
      }
      checkInterruption()
    })
      .then((res) => {
        xauth.clear()
        return res as string
      })
      .catch((err) => {
        xauth.clear()
        throw err
      })
  }

  private _checkResult() {
    const data = localStorage.getItem(X_AUTH_KEY)
    if (!data) {
      return false
    }
    const msg: AuthMessage<string> = JSON.parse(data)
    if (msg.kind != 'XAuth') {
      console.warn(
        `WARNING: expected kind 'XAuth', got '${msg.kind}' (ignoring)`
      )
      return false
    }

    localStorage.removeItem(X_AUTH_KEY)
    if (msg.error) {
      this._onFailure(msg.error)
    } else if (msg.result) {
      this._onSuccess(msg.result)
    }

    return true
  }

  clear() {
    this._authWindow?.close()
    this._authWindow = null
    if (this._timeoutId) {
      clearTimeout(this._timeoutId)
      this._timeoutId = null
    }
  }
}

export interface ChallengeInput {
  principal: Uint8Array
  message: Uint8Array
}

const challengeClient = createRequest(X_AUTH_CHALLENGE_ENDPIONT, {
  headers: {}
})

export async function challengeToken(input: ChallengeInput, kind: string) {
  const res: SuccessResponse<Uint8Array> = await challengeClient.post(
    kind,
    input
  )
  return res.result
}
