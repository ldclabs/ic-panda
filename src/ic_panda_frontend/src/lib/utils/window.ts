import { browser } from '$app/environment'
import { isNullish } from '@dfinity/utils'

const STR_UNDEFINED = 'undefined'

// NOTE: Use the function to guarantee it's re-evaluated between jsdom and node runtime for tests.
export const isWindowDefined = typeof window != STR_UNDEFINED
export const isDocumentDefined = typeof document != STR_UNDEFINED
export const hasRequestAnimationFrame = () =>
  isWindowDefined && typeof window['requestAnimationFrame'] != STR_UNDEFINED
export const noop = () => {}

let online = true
export const isOnline = () => online

// For node and React Native, `add/removeEventListener` doesn't exist on window.
const [onWindowEvent, offWindowEvent] =
  isWindowDefined && window.addEventListener
    ? [
        window.addEventListener.bind(window),
        window.removeEventListener.bind(window)
      ]
    : [noop, noop]

export const isVisible = () => {
  const visibilityState = isDocumentDefined && document.visibilityState
  return visibilityState == null || visibilityState !== 'hidden'
}

export const initFocus = (callback: () => void) => {
  if (isDocumentDefined) {
    document.addEventListener('visibilitychange', callback)
  }
  onWindowEvent('focus', callback)
  return () => {
    if (isDocumentDefined) {
      document.removeEventListener('visibilitychange', callback)
    }
    offWindowEvent('focus', callback)
  }
}

export const initReconnect = (
  onlineCallback: () => void = noop,
  offlineCallback: () => void = noop
) => {
  const onOnline = () => {
    online = true
    onlineCallback()
  }
  const onOffline = () => {
    online = false
    offlineCallback()
  }
  onWindowEvent('online', onOnline)
  onWindowEvent('offline', onOffline)
  return () => {
    offWindowEvent('online', onOnline)
    offWindowEvent('offline', onOffline)
  }
}

export const popupCenter = ({
  width,
  height
}: {
  width: number
  height: number
}): string => {
  if (!browser) {
    return ''
  }

  if (isNullish(window) || isNullish(window.top)) {
    return ''
  }

  const {
    top: { innerWidth, innerHeight }
  } = window

  const y = innerHeight / 2 + screenY - height / 2
  const x = innerWidth / 2 + screenX - width / 2

  return `toolbar=no, location=no, directories=no, status=no, menubar=no, scrollbars=yes, resizable=no, copyhistory=no, width=${width}, height=${height}, top=${y}, left=${x}`
}
