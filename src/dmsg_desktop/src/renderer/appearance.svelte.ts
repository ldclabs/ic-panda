/**
 * The appearance the person picked: the system's, light or dark. The main
 * process keeps it and sets `nativeTheme`, so styles follow through
 * `prefers-color-scheme`; this window only remembers the choice to show it.
 */
import type { Appearance } from '../shared/appearance'

let current = $state<Appearance>(window.dmsg.appearance)

export const appearance = () => current

export async function setAppearance(next: Appearance) {
  await window.dmsg.setAppearance(next)
  current = next
}
