/** Secret-package/1: complete validation before any field or attachment is exposed. */
import { ShareError, sha256Hex } from './decrypt'
export const PACKAGE_MIME = 'application/vnd.dmsg.secret-package'
export type PackageField = { id: string; type: string; label: string; section: string; value: string }
export type SecretPackage = {
  format: 'dmsg-secret-package/1'
  snapshot_id: string
  created_at: number
  item: { type: string; title: string; fields: PackageField[]; notes?: string }
  attachments: { name: string; mime: string; size: number; sha256: string; bytes: Uint8Array }[]
}
const bad = (): never => { throw new ShareError('INTEGRITY_FAILED', 'secret package') }
const encoder = new TextEncoder()
const text = (v: unknown, max: number): v is string => typeof v === 'string' && [...v].length <= max && ![...v].some((c) => { const n = c.codePointAt(0)!; return n >= 0xd800 && n <= 0xdfff })
const object = (v: any, fields: string[], optional: string[] = []) => v && typeof v === 'object' && !Array.isArray(v) && Object.keys(v).every((k) => fields.includes(k) || optional.includes(k)) && fields.every((k) => Object.hasOwn(v, k))

/** JSON.parse silently overwrites duplicate keys. Validate the token tree first. */
function strictJson(source: string): any {
  const token = /\s*("(?:\\["\\/bfnrt]|\\u[0-9a-fA-F]{4}|[^"\\\u0000-\u001f])*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\]:,])/y
  let at = 0
  let next: string | undefined
  const read = () => { token.lastIndex = at; const m = token.exec(source); if (!m) bad(); at = token.lastIndex; return m![1]! }
  const take = () => { const t = next ?? read(); next = undefined; return t }
  const peek = () => (next ??= read())
  function value(depth: number) {
    if (depth > 16) bad()
    const t = take()
    if (t === '{') {
      const keys = new Set<string>()
      if (peek() === '}') { take(); return }
      for (;;) {
        const key = take(); if (!key.startsWith('"')) bad()
        const parsed = JSON.parse(key); if (keys.has(parsed)) bad(); keys.add(parsed)
        if (take() !== ':') bad(); value(depth + 1)
        const end = take(); if (end === '}') return; if (end !== ',') bad()
      }
    }
    if (t === '[') {
      if (peek() === ']') { take(); return }
      for (;;) { value(depth + 1); const end = take(); if (end === ']') return; if (end !== ',') bad() }
    }
    if (!t.startsWith('"') && !/^(true|false|null|-?\d)/.test(t)) bad()
  }
  value(0)
  if (next !== undefined || source.slice(at).trim()) bad()
  try { return JSON.parse(source) } catch { return bad() }
}

export async function parseSecretPackage(bytes: Uint8Array): Promise<SecretPackage> {
  if (bytes.length < 4 || bytes.length > 100 * 1024 * 1024) bad()
  const length = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0)
  if (length > 256 * 1024 || length + 4 > bytes.length) bad()
  let m: any
  try { m = strictJson(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(4, 4 + length))) } catch { return bad() }
  if (!object(m, ['format', 'snapshot_id', 'created_at', 'item', 'attachments']) || m.format !== 'dmsg-secret-package/1' || !/^[0-9a-f]{64}$/.test(m.snapshot_id) || !Number.isSafeInteger(m.created_at) || m.created_at < 0) bad()
  if (!object(m.item, ['type', 'title', 'fields'], ['notes']) || !['login','api','key','ssh','wallet','card','identity','server','database','license','note'].includes(m.item.type) || typeof m.item.title !== 'string' || !text(m.item.title.trim(), 200) || !m.item.title.trim() || !Array.isArray(m.item.fields) || m.item.fields.length > 64 || (m.item.notes !== undefined && !text(m.item.notes, 20000))) bad()
  const ids = new Set<string>()
  let contentBytes = encoder.encode(m.item.notes ?? '').length
  for (const f of m.item.fields) {
    if (!object(f, ['id','type','label','section','value']) || !/^[a-z0-9_]{1,40}$/.test(f.id) || ids.has(f.id) || !['text','concealed','url','email','phone','date','month','otp'].includes(f.type) || !text(f.label, 100) || !text(f.section, 100) || !text(f.value, 10000)) bad()
    ids.add(f.id); contentBytes += encoder.encode(f.label + f.section + f.value).length
  }
  if (contentBytes > 65536 || !Array.isArray(m.attachments) || m.attachments.length > 20) bad()
  let offset = 4 + length
  const attachments: SecretPackage['attachments'] = []
  for (const a of m.attachments) {
    if (!object(a, ['name','mime','size','sha256']) || !text(a.name, 1024) || !a.name || a.name.includes('\0') || encoder.encode(a.name).length > 1024 || !text(a.mime, 256) || encoder.encode(a.mime).length > 256 || /[\u0000-\u001f\u007f]/.test(a.mime) || !Number.isSafeInteger(a.size) || a.size < 0 || a.size > 100 * 1024 * 1024 || !/^[0-9a-f]{64}$/.test(a.sha256) || offset + a.size > bytes.length) bad()
    const part = bytes.subarray(offset, offset + a.size)
    if (await sha256Hex(part) !== a.sha256) bad()
    attachments.push({ ...a, bytes: part.slice() }); offset += a.size
  }
  if (offset !== bytes.length) bad()
  return { ...m, attachments }
}

/** Suggestions only. A sender never supplies a filesystem path to save to. */
export function attachmentName(name: string): string {
  const safe = name.split(/[\\/]/).pop()?.replace(/[\u0000-\u001f\u007f]/g, '').slice(0, 200)
  return safe && safe !== '.' && safe !== '..' ? safe : 'attachment.bin'
}

/** Recognize the format even if a sender supplied the wrong outer media type. */
export function isSecretPackage(bytes:Uint8Array):boolean {
  if(bytes.length<4)return false
  const length=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength).getUint32(0)
  if(length>256*1024||length+4>bytes.length)return false
  try{return strictJson(new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(4,4+length)))?.format==='dmsg-secret-package/1'}catch{return false}
}
