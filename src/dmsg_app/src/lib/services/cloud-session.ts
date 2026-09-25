import type { AccountClient } from './account'
import { RelayError, type CloudClient } from './relay'
import { signCloudCommand, type CloudAction, type CloudContext } from '../protocol/cloud'
import { id } from '../protocol/codec'

export type AccountState = Awaited<ReturnType<AccountClient['refresh']>>
/** One account's relay context: published security evidence, request deadlines
 * bounded by that evidence, and device-signed requests. */
export class CloudSession {
  state: AccountState | null = null
  constructor(
    readonly account: AccountClient,
    readonly cloud: CloudClient,
    readonly accountId: string
  ) {}
  readonly sign = (bytes: Uint8Array) => this.account.crypto.call('deviceSign', bytes)
  async refresh() {
    for (let attempt = 0; ; attempt++) {
      const state = await this.account.refresh(this.accountId)
      try {
        await this.adopt(state)
        return state
      } catch (error) {
        if (!(error instanceof RelayError && error.code === 'POLICY_STALE') || attempt >= 2)
          throw error
        // Independent replicas can return a still-valid but older certificate.
        // Obtain another proof; never relax the relay's high-water mark.
        await new Promise((done) => setTimeout(done, 200 * (attempt + 1)))
      }
    }
  }
  /** Uses account state the caller has just verified. */
  async adopt(state: AccountState) {
    this.state = state
    await this.cloud.publishSecurity(state.verified.evidence)
  }
  async context(requestId = id()): Promise<CloudContext> {
    if (!this.state || this.state.verified.expiresAt < Date.now() + 10000) await this.refresh()
    const state = this.state!
    return {
      accountId: this.accountId,
      issuer: state.info.issuer,
      deviceId: this.account.meta.deviceId,
      securityEpoch: state.verified.securityEpoch,
      requestId,
      deadline: Math.min(Date.now() + 45000, state.verified.expiresAt)
    }
  }
  async get(path: string): Promise<any> {
    try {
      return await this.cloud.get(path, await this.context(), this.sign)
    } catch (error) {
      if (!(error instanceof RelayError && error.code === 'POLICY_STALE')) throw error
      await this.refresh()
      return this.cloud.get(path, await this.context(), this.sign)
    }
  }
  /** GET that reports an absent resource as null. */
  async find(path: string): Promise<any> {
    try {
      return await this.get(path)
    } catch (error) {
      if (error instanceof RelayError && error.code === 'NOT_FOUND') return null
      throw error
    }
  }
  async post(
    path: string,
    action: CloudAction,
    payload: Record<string, unknown>,
    requestId = id()
  ): Promise<any> {
    const context = await this.context(requestId)
    return this.cloud.post(
      path,
      await signCloudCommand(context, action, payload, this.sign),
      context,
      this.sign
    )
  }
}
