import type { PrincipalDocument } from 'agent-protocols/delegation'

export function httpsUrl(value: string): boolean {
  try {
    const url = new URL(value)
    return value === value.trim() && !/[\u0000-\u0020\u007f]/.test(value) && new TextEncoder().encode(value).length <= 2048 && url.protocol === 'https:' && !!url.hostname && !url.username && !url.password
  } catch { return false }
}

export function profileText(value: string, max: number, multiline = false): boolean {
  return [...value].length <= max && !(multiline ? /[\u0000-\u0009\u000b-\u001f\u007f-\u009f]/ : /[\u0000-\u001f\u007f-\u009f]/).test(value)
}

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`
  if (value && typeof value === 'object') return `{${Object.entries(value).filter(([, v]) => v !== undefined).sort(([a], [b]) => a.localeCompare(b)).map(([key, v]) => `${JSON.stringify(key)}:${canonical(v)}`).join(',')}}`
  return JSON.stringify(value) ?? 'null'
}

/** Compare editable fields only; timestamps and authority metadata are not a user edit. */
export function profileChanged(before: PrincipalDocument, after: PrincipalDocument): boolean {
  const fields = (document: PrincipalDocument) => {
    const extra = { ...document.extra }
    const images = extra['dmsg.link_images']
    if (images && typeof images === 'object' && !Object.keys(images).length) delete extra['dmsg.link_images']
    return {
      type: document.type ?? 'person', name: document.name?.trim() || '', description: document.description?.trim() || '',
      avatar_url: document.avatar_url || '', links: document.links ?? [], extra
    }
  }
  return canonical(fields(before)) !== canonical(fields(after))
}
