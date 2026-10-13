/**
 * Interface text in the six languages of `shared/locales.ts`. The main
 * process passes each window the language it resolved and tells every window
 * when the person picks another in the main window. `t` reads the current
 * language, so whatever a template shows follows a switch; text a view
 * already composed into its state (an error, a notice) stays as it was.
 */
import { defaultLocale, localeDir, type Locale } from '../../shared/locales'
import en, { type Catalog, type Label } from './messages/en'
import zh from './messages/zh'
import ru from './messages/ru'
import ar from './messages/ar'
import fr from './messages/fr'
import es from './messages/es'

export type { Label }

const catalogs: Record<Locale, Catalog> = { en, zh, ru, ar, fr, es }

let current = $state<Locale>(defaultLocale)

function apply(next: Locale) {
  current = next
  document.documentElement.lang = next === 'zh' ? 'zh-Hans' : next
  document.documentElement.dir = localeDir(next)
}

apply(window.dmsg.locale)
window.dmsg.onLocale(apply)

/** The current interface language. */
export const locale = () => current

/** Keep the language the person picked; the main process passes it to every window. */
export async function setLocale(next: Locale) {
  await window.dmsg.setLocale(next)
  apply(next)
}

/** A label in the current language, its `{name}` placeholders filled from `params`. */
export function t(key: Label, params?: Record<string, string | number>): string {
  const text = catalogs[current].messages[key]
  if (!params) return text
  return text.replace(/\{(\w+)\}/g, (match, name: string) => (Object.hasOwn(params, name) ? String(params[name]) : match))
}

/** An approval state for people; unknown states show as they are. */
export function stateLabel(state: string): string {
  const states: Record<string, string> = catalogs[current].approvalStates
  return states[state] ?? state
}

/** A daemon session state for people; unknown states show as they are. */
export function sessionLabel(state: string): string {
  const states: Record<string, string> = catalogs[current].sessionStates
  return states[state] ?? state
}
