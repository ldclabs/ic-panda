/**
 * Work the library has under way, shown as cards above the list: files
 * being added (the main process names them, the daemon's `files/progress`
 * counts their chunks), backups and fetches. A picked folder is one card
 * that counts the files arriving from under it. A card stays a moment when
 * it is done or cancelled and until dismissed when it failed.
 */
export type Kind = 'add' | 'backup' | 'fetch' | 'verify'
export type Stage = 'running' | 'done' | 'error' | 'cancelled'

export interface Job {
  key: number
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
  /** A cancel was asked for. */
  cancelling?: boolean
}

export const jobs = $state<Job[]>([])

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
    if (!running) start('add', name)
    const job = running ?? jobs[jobs.length - 1]
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
 * up or fetched, by any client. The last chunk ends the card; a file from
 * under a picked folder counts on the folder's card instead. Other work,
 * such as a share link's edition, shows where it was asked for.
 */
export function progress(data: { job?: string; path?: string; done?: number; total?: number }) {
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
  let job = jobs.find((j) => j.kind === kind && j.stage === 'running' && j.name === name)
  job ??= start(kind, name)
  job.done = done
  job.total = total
  if (job.total > 0 && job.done >= job.total) settle(job, 'done')
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

/** An import failed or was cancelled: every file still being added is marked. */
export function failed(code: string, cancelled = false) {
  for (const job of jobs) if (job.kind === 'add' && job.stage === 'running') settle(job, cancelled ? 'cancelled' : 'error', code)
}

/** Backups the daemon reports in flight, from `files/backup_status`. */
export function backups(list: { file: string; version: string; sent: number; chunks: number; error?: string | null; committed?: boolean }[]) {
  for (const entry of list) {
    let job = jobs.find((j) => j.kind === 'backup' && j.name === entry.file && j.stage !== 'done')
    job ??= start('backup', entry.file)
    job.version = entry.version
    job.done = entry.sent
    job.total = entry.chunks
    if (entry.error) {
      job.stage = 'error'
      job.error = entry.error
    } else if (job.stage === 'error') job.stage = 'running'
  }
  for (const job of jobs) if (job.kind === 'backup' && job.stage !== 'done' && !list.some((e) => e.file === job.name)) settle(job, 'done')
}
