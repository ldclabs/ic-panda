import type { Messages } from '$lib/i18n/messages/en'

export const REPO = 'https://github.com/ldclabs/ic-panda'
// The repository's latest release is always a stable dMsg client release, so these
// links follow each new version without a change here.
export const RELEASE = `${REPO}/releases/latest`
export const MAC_APP = `${REPO}/releases/latest/download/dMsg-mac-arm64.dmg`
export const WINDOWS_APP = `${REPO}/releases/latest/download/dMsg-win-x64.exe`
export const APP_STORE = 'https://apps.apple.com/app/id6827050680'
/** The landing page's section listing every platform. */
const ALL_DOWNLOADS = '/#release'

export type Install = { href: string; label: keyof Messages }

/** The install that fits a device, or the list of them when none does yet. */
export function installFor(userAgent: string, touchPoints: number): Install {
  // iPadOS asks for desktop sites and reads as a Mac, apart from its touch screen.
  const ios =
    /iPhone|iPad|iPod/.test(userAgent) ||
    (/Macintosh/.test(userAgent) && touchPoints > 1)
  if (ios) return { href: APP_STORE, label: 'Download on the App Store' }
  if (/Android/.test(userAgent))
    return { href: ALL_DOWNLOADS, label: 'Download dMsg' }
  if (/Macintosh/.test(userAgent))
    return { href: MAC_APP, label: 'Download for Mac' }
  if (/Windows/.test(userAgent))
    return { href: WINDOWS_APP, label: 'Windows installer' }
  return { href: ALL_DOWNLOADS, label: 'Download dMsg' }
}
