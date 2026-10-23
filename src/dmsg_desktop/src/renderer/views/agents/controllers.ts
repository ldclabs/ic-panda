import { validateAudience } from 'agent-protocols/delegation'

/** Exact comma-separated entries: empty entries and duplicates are errors. */
export const controllerEntries = (text: string) => text.split(',').map((entry) => entry.trim())

const bytes = (text: string) => new TextEncoder().encode(text).length

/** The same bounded ceiling the client accepts before creating an approval. */
export function controllerErrors(name: string, scopes: string, audiences: string, unrestricted = false) {
  const errors: { name?: 'controllerNameInvalid'; scopes?: 'controllerScopesInvalid'; audiences?: 'controllerAudiencesInvalid' } = {}
  if (bytes(name.trim()) > 64 || /[\p{Cc}]/u.test(name)) errors.name = 'controllerNameInvalid'
  if (unrestricted) return errors
  const s = controllerEntries(scopes), a = controllerEntries(audiences)
  if (s.length > 8 || new Set(s).size !== s.length || s.some((v) => !v || bytes(v) > 64 || /[\s*\p{Cc}]/u.test(v))) errors.scopes = 'controllerScopesInvalid'
  if (a.length > 4 || new Set(a).size !== a.length || a.some((v) => {
    if (bytes(v) > 256) return true
    try { validateAudience(v); return false } catch { return true }
  })) errors.audiences = 'controllerAudiencesInvalid'
  return errors
}

/** Caller must validate first; empty fields never stand for unrestricted. */
export function controllerParams(name: string, scopes: string, audiences: string, unrestricted = false) {
  return {
    name: name.trim() || null,
    ...(unrestricted ? { unrestricted: true } : { scopes: controllerEntries(scopes), audiences: controllerEntries(audiences) })
  }
}
