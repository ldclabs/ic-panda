import { writable, type Writable } from 'svelte/store'

/**
 * Light/dark mode, ported from Skeleton's lightswitch utility.
 *
 * The `localStorage` keys and their meaning are kept exactly as they were
 * (`true` is light, `false` is dark, JSON-encoded), so a returning visitor
 * lands on the mode they last chose rather than being reset by the upgrade.
 */

const KEY_USER_PREFERS = 'modeUserPrefers'
const KEY_CURRENT = 'modeCurrent'

function read(key: string): boolean | undefined {
  try {
    const raw = globalThis.localStorage?.getItem(key)
    return raw === null || raw === undefined ? undefined : JSON.parse(raw)
  } catch {
    return undefined
  }
}

function write(key: string, value: boolean) {
  try {
    globalThis.localStorage?.setItem(key, JSON.stringify(value))
  } catch {
    // Private mode, or storage disabled: the mode simply does not persist.
  }
}

/** TRUE: light, FALSE: dark. */
export const modeCurrent: Writable<boolean> = writable(
  read(KEY_CURRENT) ?? true
)

export function getModeOsPrefers(): boolean {
  return (
    globalThis.matchMedia?.('(prefers-color-scheme: light)').matches ?? true
  )
}

/** Applies the mode to the document and remembers it. */
export function setModeCurrent(value: boolean) {
  const classes = document.documentElement.classList
  if (value) classes.remove('dark')
  else classes.add('dark')
  modeCurrent.set(value)
  write(KEY_CURRENT, value)
}

export function setModeUserPrefers(value: boolean) {
  write(KEY_USER_PREFERS, value)
}

/**
 * Applies the stored mode on first paint. An explicit choice wins; otherwise
 * we follow the OS.
 */
export function setInitialClassState() {
  const classes = document.documentElement.classList
  const prefers = read(KEY_USER_PREFERS)
  const dark =
    prefers === false ||
    (prefers === undefined &&
      (globalThis.matchMedia?.('(prefers-color-scheme: dark)').matches ??
        false))

  if (dark) classes.add('dark')
  else classes.remove('dark')
  modeCurrent.set(!dark)
}
