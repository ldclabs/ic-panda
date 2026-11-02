/**
 * Work the library has under way, shown as cards above the list: files
 * being added (the main process names them, the daemon's `files/progress`
 * counts their chunks), backups and fetches. A picked folder is one card
 * that counts the files arriving from under it. A card stays a moment when
 * it is done or cancelled and until dismissed when it failed.
 */
export type Kind = 'add' | 'backup' | 'fetch' | 'verify'
export type Stage = 'running' | 'done' | 'error' | 'cancelled' | 'paused' | 'retrying' | 'committing' | 'verifying' | 'queued'

export interface Job {
  key: number
  upload?: string
  bytes?: number
  totalBytes?: number
  speed?: number
  sampled?: number
  retryAt?: number
  kind: Kind
  /** The file's name, or the path the daemon reports; a backup's file ID; a fetch's upload ID. */
  name: string
  /** A backup's version, which cancelling it names. */
  version?: string
  done: number
  total: number
  stage: Stage
  error?: string
  /** A note shown when done, such as a duplicate's count. */
  note?: string
  /** A picked folder's path, which its files' progress paths start with. */
  folder?: string
  /** The files of a picked folder added so far, and the one arriving. */
  files?: number
  current?: string
  /**
   * An import this window started, which Cancel stops and its failure
   * ends; work that only reports progress, such as a version saved from
   * an opened copy, is neither.
   */
  cancellable?: boolean
  /** A cancel was asked for. */
  cancelling?: boolean
}

export const jobs = $state<Job[]>([])
let accountScope: string | null = null

/** Navigation keeps the account's cards; changing account forgets them. */
export function scopeJobs(account: string | null) {
  if (accountScope !== account) { jobs.splice(0); accountScope = account }
}

let last = 0

const basename = (path: string) => path.split(/[\\/]/).pop() ?? path
const under = (path: string, folder: string) => path.startsWith(`${folder}/`) || path.startsWith(`${folder}\\`)

function start(kind: Kind, name: string): Job {
  jobs.push({ key: ++last, kind, name, done: 0, total: 0, stage: 'running' })
  return jobs[jobs.length - 1]
}

export function dismiss(key: number) {
  const index = jobs.findIndex((job) => job.key === key)
  if (index >= 0) jobs.splice(index, 1)
}

function settle(job: Job, stage: Stage, error?: string) {
  job.stage = stage
  job.error = error
  if (stage === 'done' || stage === 'cancelled') setTimeout(() => dismiss(job.key), 6000)
}

/** The main process named what the person picked or dropped, and the paths of the folders among them. */
export function picked(names: string[], folders: string[] = []) {
  for (const name of names) {
    const folder = folders.find((path) => basename(path) === name)
    const running = jobs.find((job) => job.kind === 'add' && job.stage === 'running' && job.name === name)
    const job = running ?? start('add', name)
    job.cancellable = true
    if (folder && !job.folder) Object.assign(job, { folder, files: 0 })
  }
}

/** The jobs `files/progress` names; without a name it is an import. */
const kinds = new Map<string, Kind>([
  ['backup', 'backup'],
  ['fetch', 'fetch'],
  ['verify', 'verify']
])

/**
 * The daemon's `files/progress`: chunks done of a file being added, backed
 * up or fetched, by any client. Cloud work waits for a confirmed result; a file from
 * under a picked folder counts on the folder's card instead. Other work,
 * such as a share link's edition, shows where it was asked for.
 */
export function progress(data: { job?: string; path?: string; done?: number; total?: number; upload_id?: string; stage?: string; bytes?: number; total_bytes?: number; error?: string | null; retry_at?: number }) {
  const kind = data.job ? kinds.get(data.job) : 'add'
  if (!kind || data.path == null) return
  const path = String(data.path)
  const done = Number(data.done ?? 0)
  const total = Number(data.total ?? 0)
  if (kind === 'add') {
    const parent = jobs.find((j) => j.kind === 'add' && j.stage === 'running' && j.folder && under(path, j.folder))
    if (parent) {
      parent.current = basename(path)
      if (total > 0 && done >= total) parent.files = (parent.files ?? 0) + 1
      return
    }
  }
  const name = kind === 'add' ? basename(path) : path
  let job = jobs.find((j) => j.kind === kind && (data.upload_id ? j.upload === data.upload_id : j.name === name) && j.stage !== 'done' && j.stage !== 'cancelled')
  job ??= start(kind, name)
  job.done = done
  job.total = total
  if (kind === 'add') {
    if (job.total > 0 && job.done >= job.total) settle(job, 'done')
  } else {
    job.upload = data.upload_id ?? job.upload
    const stage = stageOf(data.stage)
    if (stage === 'done') settle(job, 'done')
    else job.stage = stage
    job.error = data.error ?? undefined
    job.retryAt = data.retry_at
    measure(job, data.bytes, data.total_bytes)
  }
}

/**
 * The daemon answered an import of what the person picked (`names`): each
 * file added, with how many files already hold its bytes and whether it
 * joined a file of its name as a version. A picked folder's card ends with
 * the answer and its count; the files within get no cards of their own.
 */
export function added(
  names: string[],
  results: { file?: { name?: string; versions?: number }; duplicate_of?: string[] }[],
  notes: { duplicate: (n: number) => string; version: string; files: (n: number) => string }
) {
  for (const result of results) {
    const name = result.file?.name ?? ''
    if (!names.includes(name)) continue
    // Its last chunk may have ended the card already; the newest card of the name is its.
    const job = jobs.findLast((j) => j.kind === 'add' && j.stage !== 'error' && !j.folder && j.name === name)
    if (!job) continue
    job.done = job.total = Math.max(job.total, 1)
    job.note = result.duplicate_of?.length ? notes.duplicate(result.duplicate_of.length) : (result.file?.versions ?? 1) > 1 ? notes.version : undefined
    if (job.stage === 'running') settle(job, 'done')
  }
  for (const job of jobs) {
    if (job.kind !== 'add' || job.stage !== 'running' || !names.includes(job.name)) continue
    if (job.folder) job.note = notes.files(job.files ?? 0)
    settle(job, 'done')
  }
}

/** What was named but never imported, such as a drop whose question was dismissed, leaves its cards. */
export function forget(names: string[]) {
  for (const job of jobs.filter((j) => j.kind === 'add' && j.stage === 'running' && names.includes(j.name))) dismiss(job.key)
}

/** An import failed or was cancelled: every file this window is still adding is marked. */
export function failed(code: string, cancelled = false) {
  for (const job of jobs) if (job.kind === 'add' && job.stage === 'running' && job.cancellable) settle(job, cancelled ? 'cancelled' : 'error', code)
}

const stageOf = (stage?: string): Stage => stage === 'uploading' || stage === 'downloading' ? 'running' : ['queued', 'paused', 'retrying', 'committing', 'verifying', 'done'].includes(stage ?? '') ? stage as Stage : stage === 'failed' ? 'error' : 'running'

function measure(job: Job, bytes?: number, total?: number) {
  if (bytes == null) return
  const now = Date.now()
  if (job.sampled && bytes >= (job.bytes ?? 0) && now > job.sampled) {
    const speed = 1000 * (bytes - (job.bytes ?? 0)) / (now - job.sampled)
    job.speed = speed
  }
  job.bytes = bytes; job.totalBytes = total; job.sampled = now
}

/** Only the daemon's final state settles cloud work; byte counts never do. */
export function backups(list: { file: string; version: string; upload_id: string; sent: number; chunks: number; stage?: string; error?: string | null; committed?: boolean; bytes?: number; total_bytes?: number; retry_at?: number }[]) {
  for (const entry of list) {
    let job = jobs.find((j) => j.kind === 'backup' && j.upload === entry.upload_id)
    job ??= start('backup', entry.file)
    job.upload = entry.upload_id; job.version = entry.version
    job.done = entry.sent; job.total = entry.chunks
    job.stage = stageOf(entry.stage ?? (entry.error ? 'failed' : entry.committed ? 'committing' : 'uploading'))
    job.error = entry.error ?? undefined; job.retryAt = entry.retry_at
    measure(job, entry.bytes, entry.total_bytes)
  }
  for (const job of jobs) if (job.kind === 'backup' && job.stage !== 'done' && !list.some((e) => e.upload_id === job.upload)) settle(job, 'done')
}

export function downloads(list: { upload_id: string; verify: boolean; stage: string; done: number; total: number; bytes: number; total_bytes: number; error?: string | null; retry_at?: number }[]) {
  for (const entry of list) {
    const kind = entry.verify ? 'verify' : 'fetch'
    let job = jobs.find((j) => j.kind === kind && j.name === entry.upload_id)
    if (!job && entry.stage === 'done') continue
    job ??= start(kind, entry.upload_id)
    job.upload = entry.upload_id; job.done = entry.done; job.total = entry.total
    const stage = stageOf(entry.stage)
    if (stage === 'done' && job.stage !== 'done') settle(job, 'done')
    else job.stage = stage
    job.error = entry.error ?? undefined; job.retryAt = entry.retry_at
    measure(job, entry.bytes, entry.total_bytes)
  }
}
