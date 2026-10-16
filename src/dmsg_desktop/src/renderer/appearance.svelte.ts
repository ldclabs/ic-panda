/**
 * The appearance the person picked: the system's, light or dark. The main
 * process keeps it and sets `nativeTheme`, so styles follow through
 * `prefers-color-scheme`; this window only remembers the choice to show it.
 */
import type { Appearance } from '../shared/appearance'
import type { Label } from './i18n/index.svelte'

let current = $state<Appearance>(window.dmsg.appearance)

export const appearance = () => current

/** Each appearance's name in the menus and the command palette. */
export const appearanceLabel = { system: 'appearanceSystem', light: 'appearanceLight', dark: 'appearanceDark' } as const satisfies Record<Appearance, Label>

export async function setAppearance(next: Appearance) {
  await window.dmsg.setAppearance(next)
  current = next
}
