// from https://github.com/skeletonlabs/skeleton/blob/master/packages/skeleton/src/lib/utilities/Popup/popup.ts
//
import { get, writable, type Writable } from 'svelte/store'

/** Placement https://floating-ui.com/docs/computePosition#placement */
type Direction = 'top' | 'bottom' | 'left' | 'right'
type Placement = Direction | `${Direction}-start` | `${Direction}-end`
export interface Middleware {
  /** Offset middleware settings: https://floating-ui.com/docs/offset */
  offset?: number | Record<string, any>
  /** Shift middleware settings: https://floating-ui.com/docs/shift */
  shift?: Record<string, any>
  /** Flip middleware settings: https://floating-ui.com/docs/flip */
  flip?: Record<string, any>
  /** Size middleware settings: https://floating-ui.com/docs/size */
  size?: Record<string, any>
  /** Auto Placement middleware settings: https://floating-ui.com/docs/autoPlacement */
  autoPlacement?: Record<string, any>
  /** Hide middleware settings: https://floating-ui.com/docs/hide */
  hide?: Record<string, any>
  /** Inline middleware settings: https://floating-ui.com/docs/inline */
  inline?: Record<string, any>
}
export interface PopupSettings {
  target: string
  triggerNodeClass?: string
  /** Set the placement position. Defaults 'bottom'. */
  placement?: Placement
  /** Query elements that close the popup when clicked. Defaults `'a[href], button'`. */
  closeQuery?: string
  /** Optional callback function that reports state change. */
  state?: (state: PopupState) => void
  /** Provide Floating UI middleware settings. */
  middleware?: Middleware
}

export interface PopupState {
  open: boolean
  triggerNode: HTMLElement | null
  meta?: any
}

// Use a store to pass the Floating UI import references
export const storePopup: Writable<any> = writable(undefined)

export function initPopup(args: PopupSettings) {
  // Floating UI Modules
  const {
    computePosition,
    offset,
    shift,
    flip,
    size,
    autoPlacement,
    hide,
    inline
  } = get(storePopup)
  // Local State
  const popupState: PopupState = {
    open: false,
    triggerNode: null
  }

  const documentationLink = 'https://www.skeleton.dev/utilities/popups'
  // Elements
  let elemPopup: HTMLElement

  function setDomElements(): void {
    if (!elemPopup) {
      elemPopup =
        document.querySelector(`[data-popup="${args.target}"]`) ??
        document.createElement('div')
    }
  }

  // Render Floating UI Popup
  function render(): void {
    if (!popupState.triggerNode) return
    // Error handling for required Floating UI modules
    if (!elemPopup)
      throw new Error(
        `The data-popup="${args.target}" element was not found. ${documentationLink}`
      )
    if (!computePosition)
      throw new Error(
        `Floating UI 'computePosition' not found for data-popup="${args.target}". ${documentationLink}`
      )
    if (!offset)
      throw new Error(
        `Floating UI 'offset' not found for data-popup="${args.target}". ${documentationLink}`
      )
    if (!shift)
      throw new Error(
        `Floating UI 'shift' not found for data-popup="${args.target}". ${documentationLink}`
      )
    if (!flip)
      throw new Error(
        `Floating UI 'flip' not found for data-popup="${args.target}". ${documentationLink}`
      )

    // Bundle optional middleware
    const optionalMiddleware = []
    // https://floating-ui.com/docs/size
    if (size) optionalMiddleware.push(size(args.middleware?.size))
    // https://floating-ui.com/docs/autoPlacement
    if (autoPlacement)
      optionalMiddleware.push(autoPlacement(args.middleware?.autoPlacement))
    // https://floating-ui.com/docs/hide
    if (hide) optionalMiddleware.push(hide(args.middleware?.hide))
    // https://floating-ui.com/docs/inline
    if (inline) optionalMiddleware.push(inline(args.middleware?.inline))

    // Floating UI Compute Position
    // https://floating-ui.com/docs/computePosition
    computePosition(popupState.triggerNode, elemPopup, {
      placement: args.placement ?? 'bottom',
      // Middleware - NOTE: the order matters:
      // https://floating-ui.com/docs/middleware#ordering
      middleware: [
        // https://floating-ui.com/docs/offset
        offset(args.middleware?.offset ?? 8),
        // https://floating-ui.com/docs/shift
        shift(args.middleware?.shift ?? { padding: 8 }),
        // https://floating-ui.com/docs/flip
        flip(args.middleware?.flip),
        // Implement optional middleware
        ...optionalMiddleware
      ]
    }).then(({ x, y, placement, middlewareData }: any) => {
      Object.assign(elemPopup.style, {
        left: `${x}px`,
        top: `${y}px`
      })
    })
  }

  // State Handlers
  function openOn(triggerNode: HTMLElement, meta?: any): void {
    setDomElements()

    if (!elemPopup) return
    if (popupState.triggerNode === triggerNode) {
      close()
      return
    }

    if (elemPopup.classList.contains('popup-not-ready')) return
    if (popupState.open) {
      closeElemPopup()
    }

    // Set open state to on
    popupState.open = true
    popupState.triggerNode = triggerNode
    popupState.meta = meta
    // Update render settings
    render()
    // Update the DOM
    elemPopup.setAttribute
    elemPopup.style.display = 'block'
    elemPopup.style.opacity = '1'
    elemPopup.style.pointerEvents = 'auto'
    // enable popup interactions
    elemPopup.removeAttribute('inert')
    args.state && args.state(popupState)
  }

  function close(): void {
    if (!elemPopup) return
    // Set transition duration
    const cssTransitionDuration =
      parseFloat(
        window.getComputedStyle(elemPopup).transitionDuration.replace('s', '')
      ) * 1000
    setTimeout(closeElemPopup, cssTransitionDuration)
  }

  function closeElemPopup(): void {
    // Update the DOM
    elemPopup.style.opacity = '0'
    // disable popup interactions
    elemPopup.setAttribute('inert', '')

    // Set open state to off
    popupState.open = false
    popupState.triggerNode = null
    delete popupState.meta
  }

  const triggerNodeClass = args.triggerNodeClass ?? 'popup-trigger'
  function onWindowClick(event: any): void {
    if (popupState.open === false) return
    const button = event.target.closest('button')
    if (button && button.classList.contains(triggerNodeClass)) {
      return
    }
    // Return if click is the trigger element
    // if (popupState.triggerNode?.contains(event.target)) return
    // If click it outside the popup
    if (elemPopup && elemPopup.contains(event.target) === false) {
      close()
      return
    }
    // Handle Close Query State
    const closeQueryString: string =
      args.closeQuery === undefined ? 'a[href], button' : args.closeQuery
    // Return if no closeQuery is provided
    if (closeQueryString === '') return
    const closableMenuElements = elemPopup?.querySelectorAll(closeQueryString)
    closableMenuElements?.forEach((elem) => {
      if (elem.contains(event.target)) close()
    })
  }

  window.addEventListener('click', onWindowClick, true)
  // Lifecycle
  return {
    popupState,
    popupOpenOn: openOn,
    popupDestroy() {
      // Window Events
      window.removeEventListener('click', onWindowClick, true)
      if (elemPopup) closeElemPopup()
    }
  }
}
