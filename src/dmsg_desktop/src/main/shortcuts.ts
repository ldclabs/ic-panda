import { shortcutActions, validBindings, type ShortcutBindings, type ShortcutStatus } from '../shared/shortcuts.ts'
import { DaemonError } from './error.ts'

interface NativeShortcuts {
  register(key: string, callback: () => void): boolean
  unregister(key: string): void
}

/** Register before saving; a failed registration or write keeps the old bindings. */
export class Shortcuts {
  private keys = new Set<string>()
  private saving = false
  private native: NativeShortcuts
  private platform: string
  private bindings: ShortcutBindings
  private actions: Record<keyof ShortcutBindings, () => void>
  private save: (bindings: ShortcutBindings) => Promise<void>

  constructor(
    native: NativeShortcuts,
    platform: string,
    bindings: ShortcutBindings,
    actions: Record<keyof ShortcutBindings, () => void>,
    save: (bindings: ShortcutBindings) => Promise<void>
  ) {
    this.native = native
    this.platform = platform
    this.bindings = { ...bindings }
    this.actions = actions
    this.save = save
  }

  status(): ShortcutStatus {
    return { bindings: { ...this.bindings }, registered: {
      open: !!this.bindings.open && this.keys.has(this.bindings.open),
      lock: !!this.bindings.lock && this.keys.has(this.bindings.lock)
    } }
  }

  private register(key: string): boolean {
    try {
      if (!this.native.register(key, () => {
        const action = shortcutActions.find(action => this.bindings[action] === key)
        if (action) this.actions[action]()
      })) return false
      this.keys.add(key)
      return true
    } catch { return false }
  }

  start() {
    for (const key of Object.values(this.bindings)) if (key && !this.keys.has(key)) this.register(key)
  }

  async configure(value: unknown): Promise<ShortcutStatus> {
    if (!validBindings(value, this.platform)) throw new DaemonError('INVALID_INPUT', 'Shortcut keys')
    if (value.open && value.open === value.lock) throw new DaemonError('SHORTCUT_DUPLICATE', 'Choose different shortcuts')
    if (this.saving) throw new DaemonError('VERSION_CONFLICT', 'Shortcut settings are saving')
    this.saving = true
    const next = { ...value }, added: string[] = []
    try {
      for (const key of Object.values(next)) if (key && !this.keys.has(key)) {
        if (!this.register(key)) throw new DaemonError('SHORTCUT_UNAVAILABLE', 'Shortcut could not be registered')
        added.push(key)
      }
      await this.save(next)
      this.bindings = next
      for (const key of this.keys) if (!Object.values(next).includes(key)) {
        this.native.unregister(key)
        this.keys.delete(key)
      }
      return this.status()
    } catch (error) {
      for (const key of added) { this.native.unregister(key); this.keys.delete(key) }
      throw error
    } finally { this.saving = false }
  }

  dispose() {
    for (const key of this.keys) this.native.unregister(key)
    this.keys.clear()
  }
}
