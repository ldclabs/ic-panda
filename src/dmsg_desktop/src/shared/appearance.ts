/**
 * The interface appearance: the system's, or always light or dark. The main
 * process keeps the person's choice and sets Electron's `nativeTheme`, so
 * both windows, their menus and dialogs follow it through
 * `prefers-color-scheme`.
 */

export const appearances = ['system', 'light', 'dark'] as const

export type Appearance = (typeof appearances)[number]

export function isAppearance(value: unknown): value is Appearance {
  return typeof value === 'string' && (appearances as readonly string[]).includes(value)
}
