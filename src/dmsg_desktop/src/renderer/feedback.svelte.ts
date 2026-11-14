import { call, code } from './rpc'

export const feedbackRecipient = 'dbrt466u97gecq800000'
export interface FeedbackScreenshot { file: string; version: string; name: string; size: number; operation?: string; url?: string; expires_at?: number }
export interface FeedbackRecord { id: string; revision: number; recipient: string; text: string; context: string; screenshots: FeedbackScreenshot[]; state: 'draft' | 'pending' | 'sent'; error: string | null; editable: boolean }
const newId = () => Array.from(crypto.getRandomValues(new Uint8Array(32)), n => n.toString(16).padStart(2, '0')).join('')

/** Account-scoped editing survives navigation; only sealed core records survive lock. */
class FeedbackWorkspace {
  record = $state<FeedbackRecord | null>(null)
  text = $state('')
  context = $state('')
  screenshots = $state<FeedbackScreenshot[]>([])
  loaded = $state(false)
  saving = $state(false)
  busy = $state(false)
  dirty = $state(false)
  failure = $state('')
  private account: string | null = null
  private session: string | null = null
  private currentContext = ''
  private generation = 0
  private edits = 0
  private id = newId()
  private writes: Promise<void> | null = null

  scope(account: string | null, session: string | null = null) {
    if (account === this.account && session === this.session) return
    this.generation++; this.account = account; this.session = session; this.id = newId(); this.edits = 0; this.writes = null
    this.record = null; this.text = this.context = this.failure = ''; this.screenshots = []
    this.loaded = this.saving = this.busy = this.dirty = false
  }

  private accept(value: FeedbackRecord | null) {
    this.record = value
    if (value) { this.id = value.id; this.text = value.text; this.context = value.context; this.screenshots = value.screenshots }
  }

  async load(context: string) {
    if (!this.account || this.loaded || this.busy) return
    const generation = this.generation, account = this.account
    this.busy = true
    try {
      const value = await call<FeedbackRecord | null>('feedback/get', { account })
      if (generation !== this.generation) return
      this.currentContext = context; this.context = context; this.accept(value); this.loaded = true; this.failure = ''
    } catch (e) { if (generation === this.generation) this.failure = code(e) }
    finally { if (generation === this.generation) this.busy = false }
  }

  update(text: string, screenshots = this.screenshots) {
    if (!this.loaded || this.busy || (this.record && this.record.state !== 'draft')) return
    this.text = text; this.screenshots = screenshots; this.edits++; this.dirty = true
    void this.flush()
  }

  flush(): Promise<void> {
    if (this.writes) return this.writes
    if (!this.dirty || !this.account) return Promise.resolve()
    const generation = this.generation, account = this.account
    const work = async () => {
      this.saving = true
      let reconciled = false
      try {
        while (this.dirty && generation === this.generation && account) {
          const edits = this.edits
          let value: FeedbackRecord
          try {
            value = await call<FeedbackRecord>('feedback/save', { account, id: this.id, revision: this.record?.revision ?? 0,
              text: this.text, context: this.context, files: this.screenshots.map(s => s.file) })
          } catch (e) {
            if (code(e) !== 'VERSION_CONFLICT' || reconciled) throw e
            const saved = await call<FeedbackRecord | null>('feedback/get', { account })
            if (generation !== this.generation) return
            // Resume typing after a lost save reply without replacing the authored text.
            if (!saved || saved.id !== this.id || saved.state !== 'draft') throw e
            this.record = saved; reconciled = true; continue
          }
          if (generation !== this.generation) return
          this.record = value; this.failure = ''
          if (edits === this.edits) { this.accept(value); this.dirty = false }
        }
      } catch (e) { if (generation === this.generation) this.failure = code(e) }
      finally { if (generation === this.generation) { this.saving = false; this.writes = null } }
    }
    this.writes = work()
    return this.writes
  }

  async send() {
    if (this.busy || !this.loaded) return
    const generation = this.generation, account = this.account
    this.busy = true
    await this.flush()
    if (generation !== this.generation) return
    if (this.dirty || !this.record) { this.busy = false; return }
    this.failure = ''
    try {
      const next = await call<FeedbackRecord>('feedback/send', { account, id: this.id, revision: this.record.revision })
      if (generation === this.generation) this.accept(next)
    } catch (e) {
      if (generation !== this.generation) return
      this.failure = code(e)
      const value = await call<FeedbackRecord | null>('feedback/get', { account }).catch(() => null)
      if (generation === this.generation && value) { this.accept(value); if (value.state === 'sent') this.failure = '' }
    } finally { if (generation === this.generation) this.busy = false }
  }

  async edit() {
    if (this.busy || !this.record?.editable) return
    const generation = this.generation
    this.busy = true
    try {
      const next = await call<FeedbackRecord>('feedback/edit', { account: this.account, id: this.id })
      if (generation === this.generation) { this.accept(next); this.failure = '' }
    } catch (e) { if (generation === this.generation) this.failure = code(e) }
    finally { if (generation === this.generation) this.busy = false }
  }

  async clear() {
    if (this.busy) return
    const generation = this.generation
    this.busy = true; await this.flush()
    if (generation !== this.generation) return
    try {
      if (this.record) await call('feedback/clear', { account: this.account, id: this.id, revision: this.record.revision })
      if (generation !== this.generation) return
      this.id = newId(); this.record = null; this.text = ''; this.context = this.currentContext; this.screenshots = []; this.failure = ''; this.dirty = false
    } catch (e) { if (generation === this.generation) this.failure = code(e) }
    finally { if (generation === this.generation) this.busy = false }
  }
}

export const feedback = new FeedbackWorkspace()
