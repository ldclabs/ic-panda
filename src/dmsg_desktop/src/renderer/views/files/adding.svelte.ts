/**
 * Work the library has under way, shown as cards above the list: files
 * being added (the main process names them, the daemon's `files/progress`
 * counts their chunks), backups and fetches. A card stays a moment when it
 * is done and until dismissed when it failed.
 */
export type Kind = 'add' | 'backup' | 'fetch' | 'verify'
export type Stage = 'running' | 'done' | 'error'

export interface Job {
  key: number
  kind: Kind
  /** The file's name, or the path the daemon reports. */
  name: string
  done: number
  total: number
  stage: Stage
  error?: string
  /** A note shown when done, such as a duplicate's count. */
  note?: string
}

export const jobs = $state<Job[]>([])

let last = 0

const basename = (path: string) => path.split(/[\\/]/).pop() ?? path

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
  if (stage === 'done') setTimeout(() => dismiss(job.key), 6000)
}

/** The main process named what the person picked or dropped. */
export function picked(names: string[]) {
  for (const name of names) if (!jobs.some((job) => job.kind === 'add' && job.stage === 'running' && job.name === name)) start('add', name)
}

/** The daemon's `files/progress`: chunks done of a file being added, backed up or fetched. */
export function progress(data: { job?: string; path?: string; done?: number; total?: number }) {
  const kind: Kind = data.job === 'backup' ? 'backup' : data.job === 'fetch' ? 'fetch' : data.job === 'verify' ? 'verify' : 'add'
  const name = kind === 'add' ? basename(String(data.path ?? '')) : String(data.path ?? '')
  let job = jobs.find((j) => j.kind === kind && j.stage === 'running' && j.name === name)
  job ??= start(kind, name)
  job.done = Number(data.done ?? 0)
  job.total = Number(data.total ?? 0)
  if (kind !== 'add' && job.total > 0 && job.done >= job.total) settle(job, 'done')
}

/** The daemon answered an import: each file added, with how many files already hold its bytes. */
export function added(results: { file?: { name?: string }; duplicate_of?: string[] }[], duplicateNote: (n: number) => string) {
  for (const result of results) {
    const name = result.file?.name ?? ''
    let job = jobs.find((j) => j.kind === 'add' && j.stage === 'running' && j.name === name)
    job ??= start('add', name)
    job.done = job.total = Math.max(job.total, 1)
    job.note = result.duplicate_of?.length ? duplicateNote(result.duplicate_of.length) : undefined
    settle(job, 'done')
  }
  // Names the main process announced that the daemon did not add (a folder's children carry their own names).
  for (const job of jobs) if (job.kind === 'add' && job.stage === 'running' && job.total === 0 && !results.length) settle(job, 'done')
}

/** An import failed: every file still running is marked with the code. */
export function failed(code: string) {
  for (const job of jobs) if (job.kind === 'add' && job.stage === 'running') settle(job, 'error', code)
}

/** Backups the daemon reports in flight, from `files/backup_status`. */
export function backups(list: { file: string; sent: number; chunks: number; error?: string | null; committed?: boolean }[]) {
  for (const entry of list) {
    let job = jobs.find((j) => j.kind === 'backup' && j.name === entry.file && j.stage !== 'done')
    job ??= start('backup', entry.file)
    job.done = entry.sent
    job.total = entry.chunks
    if (entry.error) {
      job.stage = 'error'
      job.error = entry.error
    } else if (job.stage === 'error') job.stage = 'running'
  }
  for (const job of jobs) if (job.kind === 'backup' && job.stage !== 'done' && !list.some((e) => e.file === job.name)) settle(job, 'done')
}
