import { browser } from '$app/environment'
import { isNullish } from '@dfinity/utils'
import debounce from 'debounce'

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

export function clickOutside(node: HTMLElement, callback: () => void = noop) {
  const handler = (ev: PointerEvent) => {
    if (!node.contains(ev.target as Node)) {
      callback()
    }
  }

  onWindowEvent('pointerup', handler)

  return () => {
    offWindowEvent('pointerup', handler)
  }
}

export function scrollOnBottom(
  node: HTMLElement,
  {
    onTop,
    onBottom,
    onMoveUp,
    onMoveDown
  }: {
    onTop?: (() => void) | undefined
    onBottom?: (() => void) | undefined
    onMoveUp?: (() => void) | undefined
    onMoveDown?: (() => void) | undefined
  }
) {
  const callTop = onTop && debounce(onTop, 618, { immediate: true })
  const callBottom = onBottom && debounce(onBottom, 618, { immediate: true })
  const callMoveUp = onMoveUp && debounce(onMoveUp, 618, { immediate: true })
  const callMoveDown =
    onMoveDown && debounce(onMoveDown, 618, { immediate: true })

  let lastScrollTop = 0
  const handler = (ev: Event) => {
    const target = ev.currentTarget as HTMLElement
    if (target.scrollTop > lastScrollTop) {
      callMoveUp && callMoveUp()
    } else {
      callMoveDown && callMoveDown()
    }

    if (target.scrollTop < lastScrollTop && target.scrollTop <= 5) {
      callTop && callTop()
    } else if (
      target.scrollTop > lastScrollTop &&
      target.clientHeight + target.scrollTop + 5 >= target.scrollHeight
    ) {
      callBottom && callBottom()
    }

    lastScrollTop = target.scrollTop
  }

  node.addEventListener('scroll', handler)
  return () => {
    node.removeEventListener('scroll', handler)
    callBottom && callBottom.clear()
    callMoveUp && callMoveUp.clear()
    callMoveDown && callMoveDown.clear()
  }
}
