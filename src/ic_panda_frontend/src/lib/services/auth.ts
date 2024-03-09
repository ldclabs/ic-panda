import { authStore, type AuthSignInParams } from '$lib/stores/auth'

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
