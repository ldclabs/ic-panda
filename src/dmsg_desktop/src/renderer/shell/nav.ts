import type { Glyph } from '../components/glyphs'
import type { Label } from '../i18n/index.svelte'

/** A view of the main window: its route, name, icon and its ⌘ shortcut. */
export interface View {
  path: string
  label: Label
  icon: Glyph
  key?: string
}

export const views = {
  reminders: { path: '/reminders', label: 'noticeTitle', icon: 'bell' },
  channels: { path: '/channels', label: 'channels', icon: 'channels', key: '1' },
  files: { path: '/files', label: 'files', icon: 'folder', key: '2' },
  vault: { path: '/vault', label: 'vault', icon: 'vault', key: '3' },
  inbox: { path: '/inbox', label: 'inbox', icon: 'inbox', key: '4' },
  approvals: { path: '/approvals', label: 'approvals', icon: 'approvals', key: '5' },
  agents: { path: '/agents', label: 'agents', icon: 'agents', key: '6' },
  plan: { path: '/plan', label: 'planAndNames', icon: 'plan', key: '7' },
  wallet: { path: '/wallet', label: 'twNav', icon: 'coins', key: '8' },
  settings: { path: '/settings', label: 'settings', icon: 'settings', key: ',' },
  legacy: { path: '/settings/legacy', label: 'legacy', icon: 'legacy' }
} satisfies Record<string, View>

/** The sidebar's groups (PRD §4); devices, recovery and migration live in Settings. */
export const groups: { label: Label; views: View[] }[] = [
  { label: 'navWork', views: [views.channels, views.files, views.vault, views.inbox] },
  { label: 'navAuthorization', views: [views.approvals, views.agents] },
  { label: 'navAccount', views: [views.plan, views.wallet] }
]

/** The views with a ⌘ shortcut, in sidebar order, so ⌘1 is the first entry there. */
export const shortcuts: View[] = [...groups.flatMap((g) => g.views), views.settings]

/** Settings' own pages, each with its route under /settings. */
export type SettingsPage = 'session' | 'security' | 'recovery' | 'devices' | 'profile' | 'browsers' | 'files' | 'notifications' | 'shortcuts' | 'appearance' | 'legacy' | 'about' | 'feedback'

export const settingsPages: { value: SettingsPage; label: Label; icon: Glyph }[] = [
  { value: 'session', label: 'sessionAccounts', icon: 'account' },
  { value: 'security', label: 'security', icon: 'security' },
  { value: 'recovery', label: 'recovery', icon: 'recovery' },
  { value: 'devices', label: 'devices', icon: 'device' },
  { value: 'profile', label: 'publicProfile', icon: 'profile' },
  { value: 'browsers', label: 'browsers', icon: 'browser' },
  { value: 'files', label: 'files', icon: 'folder' },
  { value: 'notifications', label: 'noticeSettings', icon: 'bell' },
  { value: 'shortcuts', label: 'shortcutsTitle', icon: 'system' },
  { value: 'appearance', label: 'languageAppearance', icon: 'system' },
  { value: 'legacy', label: 'legacy', icon: 'legacy' },
  { value: 'about', label: 'aboutApp', icon: 'info' },
  { value: 'feedback', label: 'feedbackTitle', icon: 'mail' }
]

/** The view a route shows; Settings' own pages keep Settings selected in the sidebar. */
export function viewOf(route: string): View {
  if (route.startsWith(views.reminders.path)) return views.reminders
  if (route.startsWith(views.legacy.path)) return views.legacy
  return shortcuts.find((view) => route.startsWith(view.path)) ?? views.channels
}

/** The sidebar entry a route belongs to. */
export const entryOf = (route: string): View => (route.startsWith(views.settings.path) ? views.settings : viewOf(route))
