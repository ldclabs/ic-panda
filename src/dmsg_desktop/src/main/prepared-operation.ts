/** A native selection is usable once, only after preparation in the same session. */
type Preparation<T> =
  | { kind: 'idle' }
  | { kind: 'preparing'; session: number }
  | { kind: 'ready'; session: number; value: T }
  | { kind: 'running'; session: number }

export class PreparedOperation<T> {
  private serial = 0
  private state: Preparation<T> = { kind: 'idle' }

  begin(): number {
    const session = ++this.serial
    this.state = { kind: 'preparing', session }
    return session
  }

  preparing(session: number): boolean {
    return this.state.kind === 'preparing' && this.state.session === session
  }

  ready(session: number, value: T): boolean {
    if (!this.preparing(session)) return false
    this.state = { kind: 'ready', session, value }
    return true
  }

  take(session: number): T | null {
    if (this.state.kind !== 'ready' || this.state.session !== session) return null
    const value = this.state.value
    this.state = { kind: 'running', session }
    return value
  }

  finish(session: number) {
    if ((this.state.kind === 'preparing' || this.state.kind === 'running') && this.state.session === session) this.cancel()
  }

  cancel() {
    this.state = { kind: 'idle' }
  }
}
