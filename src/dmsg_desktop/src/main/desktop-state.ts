/**
 * What the app keeps for itself in its profile's `desktop.json`: where the
 * main window was, the view it showed and the desktop's own choices. Nothing
 * from an account goes here: a route keeps only its view, never a query.
 */
export interface Bounds {
  x: number
  y: number
  width: number
  height: number
}

export interface DesktopState {
  bounds?: Bounds
  maximized?: boolean
  route?: string
  /** Lock dMsg when the screen locks or the computer sleeps. */
  lockOnScreenLock: boolean
  /** The first close said where dMsg goes on running. */
  closeHintShown?: boolean
}

/** A view to open again: not an approval, and no query or ID that came from a link. */
export const isViewRoute = (route: unknown): route is string =>
  typeof route === 'string' && /^\/(channels|files|vault|inbox|approvals|agents|plan|wallet|settings)(\/[a-z-]+)?$/.test(route)

const isBounds = (value: any): value is Bounds =>
  !!value && [value.x, value.y, value.width, value.height].every(Number.isFinite) && value.width > 0 && value.height > 0

/** The kept state, whatever a hand-edited or older file holds. */
export function readDesktopState(text: string): DesktopState {
  const value = JSON.parse(text)
  return {
    bounds: isBounds(value?.bounds) ? value.bounds : undefined,
    maximized: value?.maximized === true,
    route: isViewRoute(value?.route) ? value.route : undefined,
    lockOnScreenLock: value?.lockOnScreenLock !== false,
    closeHintShown: value?.closeHintShown === true
  }
}

/**
 * The kept bounds, when enough of the title bar is still on a display: a
 * monitor unplugged since would otherwise leave the window out of reach.
 */
export function boundsOnScreen(bounds: Bounds | undefined, displays: Bounds[]): Bounds | undefined {
  if (!bounds) return undefined
  const reachable = displays.some((area) => {
    const across = Math.min(bounds.x + bounds.width, area.x + area.width) - Math.max(bounds.x, area.x)
    return across >= 120 && bounds.y >= area.y && bounds.y < area.y + area.height - 40
  })
  return reachable ? bounds : undefined
}
