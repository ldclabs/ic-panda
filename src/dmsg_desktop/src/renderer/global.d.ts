import type { DesktopBridge } from '../shared/contract'

declare global {
  interface Window {
    dmsg: DesktopBridge
  }
}
export {}
