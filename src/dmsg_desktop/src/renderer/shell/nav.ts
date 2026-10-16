import type { Glyph } from '../components/glyphs'
import type { Label } from '../i18n/index.svelte'

/** A view of the main window: its route, name, icon, a line on what it holds, and its ⌘ shortcut. */
export interface View {
  path: string
  label: Label
  detail: Label
  icon: Glyph
  key?: string
}

export const views = {
  vault: { path: '/vault', label: 'vault', detail: 'vaultDetail', icon: 'vault', key: '1' },
  files: { path: '/files', label: 'files', detail: 'filesDetail', icon: 'folder', key: '2' },
  channels: { path: '/channels', label: 'channels', detail: 'channelsDetail', icon: 'channels', key: '3' },
  inbox: { path: '/inbox', label: 'inbox', detail: 'inboxDetail', icon: 'inbox', key: '4' },
  approvals: { path: '/approvals', label: 'approvals', detail: 'approvalsDetail', icon: 'approvals', key: '5' },
  agents: { path: '/agents', label: 'agents', detail: 'agentsDetail', icon: 'agents', key: '6' },
  plan: { path: '/account', label: 'planAndNames', detail: 'planDetail', icon: 'plan', key: '7' },
  wallet: { path: '/wallet', label: 'twNav', detail: 'twDetail', icon: 'coins', key: '8' },
  settings: { path: '/settings', label: 'settings', detail: 'settingsDetail', icon: 'settings', key: ',' },
  legacy: { path: '/settings/legacy', label: 'legacy', detail: 'legacyDetail', icon: 'legacy' }
} satisfies Record<string, View>

/** The sidebar's groups (PRD §4); devices, recovery and migration live in Settings. */
export const groups: { label: Label; views: View[] }[] = [
  { label: 'navWork', views: [views.vault, views.files, views.channels, views.inbox] },
  { label: 'navAuthorization', views: [views.approvals, views.agents] },
  { label: 'navAccount', views: [views.plan, views.wallet] }
]

/** The views with a ⌘ shortcut, in sidebar order. */
export const shortcuts: View[] = [...groups.flatMap((g) => g.views), views.settings]

/** The view a route shows; Settings' own pages keep Settings selected in the sidebar. */
export function viewOf(route: string): View {
  if (route.startsWith(views.legacy.path)) return views.legacy
  return shortcuts.find((view) => route.startsWith(view.path)) ?? views.vault
}

/** The sidebar entry a route belongs to. */
export const entryOf = (route: string): View => (route.startsWith(views.settings.path) ? views.settings : viewOf(route))
