import { focus as vault } from '../vault/focus.svelte'
import { focus as files } from '../files/focus.svelte'
export function openVault(id?: string) {
  vault.id = id ?? null
  location.hash = '/vault'
}
export function openFile(id?: string) {
  files.id = id ?? null
  location.hash = '/files'
}
