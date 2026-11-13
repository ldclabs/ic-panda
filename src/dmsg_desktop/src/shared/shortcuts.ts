/** Only these two application actions may be bound to a global shortcut. */
export const shortcutActions = ['open', 'lock'] as const
export type ShortcutAction = typeof shortcutActions[number]
export type ShortcutBindings = Record<ShortcutAction, string | null>
export interface ShortcutStatus {
  bindings: ShortcutBindings
  registered: Record<ShortcutAction, boolean>
}

export const defaultShortcuts = (platform: string): ShortcutBindings => ({
  open: `${platform === 'darwin' ? 'Command' : 'Control'}+Shift+Space`,
  lock: `${platform === 'darwin' ? 'Command' : 'Control'}+Shift+L`
})

const modifiers = ['Command', 'Control', 'Alt', 'Shift']
const keyPattern = /^(?:[A-Z0-9]|F(?:[1-9]|1\d|2[0-4])|Space|Tab|Enter|Escape|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Up|Down|Left|Right)$/

/** Keep IPC and stored preferences bounded to the same keys as the recorder. */
export function validShortcut(value: unknown, platform: string): value is string {
  if (typeof value !== 'string' || value.length > 80) return false
  const parts = value.split('+'), key = parts.pop() ?? ''
  return keyPattern.test(key) && parts.length > 0 && parts.some(part => part !== 'Shift')
    && parts.every(part => modifiers.includes(part) && (platform === 'darwin' || part !== 'Command'))
    && parts.join('+') === modifiers.filter(part => parts.includes(part)).join('+')
}

export function validBindings(value: unknown, platform: string): value is ShortcutBindings {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const entries = Object.entries(value)
  return entries.length === 2 && shortcutActions.every(action => Object.hasOwn(value, action))
    && entries.every(([, key]) => key === null || validShortcut(key, platform))
}

export function recordedShortcut(event: Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'isComposing'>, platform: string): string | null {
  if (event.isComposing) return null
  const names: Record<string, string> = { ' ': 'Space', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right' }
  let key = names[event.key] ?? event.key
  if (/^[a-z]$/i.test(key)) key = key.toUpperCase()
  if (!keyPattern.test(key) && /^(Key[A-Z]|Digit\d)$/.test(event.code)) key = event.code.replace(/^(Key|Digit)/, '')
  const parts = [event.metaKey && 'Command', event.ctrlKey && 'Control', event.altKey && 'Alt', event.shiftKey && 'Shift', key].filter(Boolean)
  const value = parts.join('+')
  return validShortcut(value, platform) ? value : null
}

/** Shortcut symbols remain left-to-right in every interface language. */
export function shortcutLabel(value: string | null, platform: string): string {
  if (!value) return '—'
  const names: Record<string, string> = platform === 'darwin'
    ? { Command: '⌘', Control: '⌃', Alt: '⌥', Shift: '⇧', Space: 'Space' }
    : { Control: 'Ctrl' }
  return value.split('+').map(part => names[part] ?? part).join(' + ')
}
