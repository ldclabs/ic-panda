/**
 * The appearance the person picked: the system's, light or dark. The main
 * process keeps it and sets `nativeTheme`, so styles follow through
 * `prefers-color-scheme`; this window only remembers the choice to show it.
 */
import type { Appearance } from '../shared/appearance'
import type { Label } from './i18n/index.svelte'
import { still } from './components/motion'

let current = $state<Appearance>(window.dmsg.appearance)

export const appearance = () => current

/** Each appearance's name in the menus and the command palette. */
export const appearanceLabel = { system: 'appearanceSystem', light: 'appearanceLight', dark: 'appearanceDark' } as const satisfies Record<Appearance, Label>

/**
 * Picking light or dark cross-fades the whole window (260ms) once the new
 * colours apply; under reduced motion, or for the system's own changes, they
 * switch at once.
 */
export async function setAppearance(next: Appearance) {
  const change = async () => {
    await window.dmsg.setAppearance(next)
    current = next
    await applied(next)
  }
  if (next === current || still() || !document.startViewTransition) return change()
  await document.startViewTransition(change).finished
}

/**
 * Waits for `prefers-color-scheme` to follow the main process: at most 300ms
 * for light or dark, 150ms for the system's, which may already match.
 */
function applied(next: Appearance) {
  const dark = matchMedia('(prefers-color-scheme: dark)')
  if (next !== 'system' && dark.matches === (next === 'dark')) return Promise.resolve()
  return new Promise<void>((done) => {
    const timer = setTimeout(done, next === 'system' ? 150 : 300)
    dark.addEventListener('change', () => (clearTimeout(timer), done()), { once: true })
  })
}
