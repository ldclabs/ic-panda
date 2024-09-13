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

export const isActive = () => isOnline() && isVisible()

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

export function scrollOnHooks(
  node: HTMLElement,
  {
    onTop,
    onBottom,
    onMoveUp,
    onMoveDown,
    inMoveUpViewport,
    inMoveDownViewport,
    inViewportHasId = true,
    inViewportHasClass = ''
  }: {
    onTop?: (() => void) | undefined
    onBottom?: (() => void) | undefined
    onMoveUp?: (() => void) | undefined
    onMoveDown?: (() => void) | undefined
    inMoveUpViewport?: ((els: HTMLElement[]) => void) | undefined
    inMoveDownViewport?: ((els: HTMLElement[]) => void) | undefined
    inViewportHasId?: boolean
    inViewportHasClass?: string
  }
) {
  if (!node) return noop

  const callTop = onTop && debounce(onTop, 618, { immediate: false })
  const callBottom = onBottom && debounce(onBottom, 618, { immediate: false })
  const callMoveUp = onMoveUp && debounce(onMoveUp, 618, { immediate: false })
  const callMoveDown =
    onMoveDown && debounce(onMoveDown, 618, { immediate: false })
  const callInMoveUpViewport =
    inMoveUpViewport && debounce(inMoveUpViewport, 618, { immediate: false })
  const callInMoveDownViewport =
    inMoveDownViewport &&
    debounce(inMoveDownViewport, 618, { immediate: false })

  let lastScrollTop = 0
  const handler = (ev: Event) => {
    const target = ev.currentTarget as HTMLElement
    if (target.scrollTop > lastScrollTop) {
      callMoveUp && callMoveUp()
      if (callInMoveUpViewport) {
        let children = Array.from(target.children) as HTMLElement[]
        if (inViewportHasId || inViewportHasClass) {
          children = children.filter((el) => {
            if (inViewportHasId) {
              return !!el.id
            }
            return el.classList.contains(inViewportHasClass)
          })
        }
        const els = elementsInViewport(target, children)
        if (els.length > 0) {
          callInMoveUpViewport(els)
        }
      }
    } else {
      callMoveDown && callMoveDown()
      if (callInMoveDownViewport) {
        let children = Array.from(target.children) as HTMLElement[]
        if (inViewportHasId || inViewportHasClass) {
          children = children.filter((el) => {
            if (inViewportHasId) {
              return !!el.id
            }
            return el.classList.contains(inViewportHasClass)
          })
        }
        const els = elementsInViewport(target, children)
        if (els.length > 0) {
          callInMoveDownViewport(els)
        }
      }
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
    callInMoveUpViewport && callInMoveUpViewport.clear()
    callInMoveDownViewport && callInMoveDownViewport.clear()
  }
}

export function elementsInViewport(
  container: HTMLElement,
  els: HTMLElement[]
): HTMLElement[] {
  const containerRect = container.getBoundingClientRect()
  const rt: HTMLElement[] = []
  for (const el of els) {
    const rect = el.getBoundingClientRect()
    if (
      (rect.top >= containerRect.top && rect.top < containerRect.bottom) ||
      (rect.bottom <= containerRect.bottom && rect.bottom > containerRect.top)
    ) {
      rt.push(el)
    }
  }

  return rt
}
