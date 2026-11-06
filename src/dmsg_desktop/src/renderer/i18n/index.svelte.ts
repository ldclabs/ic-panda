/// <reference types="vite/client" />
/**
 * Interface text in the languages of `shared/locales.ts`. The main
 * process passes each window the language it resolved and tells every window
 * when the person picks another in the main window. `t` reads the current
 * language, so whatever a template shows follows a switch; text a view
 * already composed into its state (an error, a notice) stays as it was.
 */
import { defaultLocale, localeDir, type Locale } from '../../shared/locales'
import { rememberFailures } from '../failure'
import en, { type Catalog, type Label } from './messages/en'

export type { Label }

const loaders = import.meta.glob<{ default: Catalog }>(['./messages/*.ts', '!./messages/en.ts'])
const catalogs: Partial<Record<Locale, Catalog>> = { en }
rememberFailures(en)

let current = $state<Locale>(defaultLocale)
let wanted: Locale = defaultLocale

/** Each catalog loads the first time its language is asked for; English is always here. */
async function loadCatalog(next: Locale) {
  if (catalogs[next]) return
  const catalog = (await loaders[`./messages/${next}.ts`]()).default
  catalogs[next] = catalog
  rememberFailures(catalog)
}

function apply(next: Locale) {
  current = next
  document.documentElement.lang = next === 'zh' ? 'zh-Hans' : next
  document.documentElement.dir = localeDir(next)
}

async function show(next: Locale) {
  wanted = next
  await loadCatalog(next)
  if (wanted === next) apply(next)
}

/** The window's language, ready before anything is drawn in it. */
export const languageReady = show(window.dmsg.locale)
window.dmsg.onLocale((next) => void show(next))

/** The current interface language. */
export const locale = () => current

/** Keep the language the person picked; the main process passes it to every window. */
export async function setLocale(next: Locale) {
  await loadCatalog(next)
  await window.dmsg.setLocale(next)
  await show(next)
}

/** A label in the current language, its `{name}` placeholders filled from `params`. */
export function t(key: Label, params?: Record<string, string | number>): string {
  const text = catalogs[current]!.messages[key]
  if (!params) return text
  return text.replace(/\{(\w+)\}/g, (match, name: string) => (Object.hasOwn(params, name) ? String(params[name]) : match))
}

/** An approval state for people; unknown states show as they are. */
export function stateLabel(state: string): string {
  const states: Record<string, string> = catalogs[current]!.approvalStates
  return states[state] ?? state
}

/** A daemon session state for people; unknown states show as they are. */
export function sessionLabel(state: string): string {
  const states: Record<string, string> = catalogs[current]!.sessionStates
  return states[state] ?? state
}

/** Dynamic vocabulary keys still use the same complete, checked catalogs. */
export function hasLabel(key: string): key is Label {
  return Object.hasOwn(en.messages, key)
}
