import type { HttpAgent } from '@icp-sdk/core/agent'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE,
  AccountCommand,
  AccountInfo,
  AccountMutation,
  AdmissionTicket,
  CreateAccount,
  RecoveryRequest
} from '../canisters/generated/user'
import type { CryptoClient } from '../crypto/client'
import type { WorkspaceMeta } from '../models'
import { DmsgError, ensure } from '../errors'
import { canonical, digest, equal, hex, id, unb64, unhex } from '../protocol/codec'
import { xidBytes, xidText } from '../protocol/identity'
import {
  accountApprovalMessage,
  accountOperationDigest,
  accountValue,
  createAccountMessage,
  decodeControl,
  deviceInput,
  encodeControl,
  recoveryDeviceMessage,
  type ControlMethod
} from '../protocol/account'
import { verifyCloudSecurity } from './cloud-security'
import { ed25519 } from '../crypto/primitives'

export interface ControlJournal {
  version: 1
  home: string
  caller: string
  account: string | null
  method: ControlMethod
  args: string
  requestId: string
  stage: 'prepared' | 'unknown' | 'confirmed' | 'rejected'
  error?: string
}
export type AccountCrypto = Pick<CryptoClient, 'call'>
export function controlResult<T>(result: { Ok: T } | { Err: unknown }): T {
  if ('Ok' in result) return result.Ok
  const [code, detail] = Object.entries(result.Err as object)[0] ?? ['UNAVAILABLE', null]
  const messages: Record<string, string> = {
    AuthRequired: '认证已过期或已解除绑定，请重新连接正确身份。',
    DeviceNotApproved: '这台设备尚未批准或已经撤销。',
    PolicyStale: '账户权限已变化，请刷新后核对。',
    VersionConflict: '另一设备已改变账户或根预留，请读取当前状态。',
    QuotaExceeded: '账户操作次数或 cycles 预算不足；不会自动提高预算或重复申请。',
    RecoveryIncomplete: '请先完成账户绑定与内容根初始化。',
    Expired: '请求或预留已过期，请查询原操作并核对当前状态。',
    ResultExpired: '原操作结果已超出保留范围；不能据此认定操作未发生。',
    Pending: '仍有未完成操作，请先查询原请求。',
    Locked: '账户处于受限状态，请检查恢复等待期。'
  }
  throw new DmsgError(
    code,
    messages[code]
      ? `${code}：${messages[code]}`
      : typeof detail === 'string'
        ? `${code}: ${detail}`
        : code
  )
}
const same = (a: Uint8Array | number[], b: Uint8Array | number[]) =>
  equal(Uint8Array.from(a), Uint8Array.from(b))
/** Serde view of a pending recovery, as the certified leaf commits to it. */
export const pendingRecoveryDigest = (pending: {
  request: RecoveryRequest
  execute_after: bigint
}) =>
  digest('dmsg/pending-recovery/v1', {
    request: accountValue(pending.request),
    execute_after: pending.execute_after
  })

/** All mutations are explicitly requested by the unlocked settings page. The
 * encrypted journal is durable before dispatch; reconnecting does not submit. */
export class AccountClient {
  readonly home: Principal
  constructor(
    readonly user: _SERVICE,
    readonly agent: HttpAgent,
    readonly caller: Principal,
    readonly crypto: AccountCrypto,
    readonly meta: WorkspaceMeta,
    home: string
  ) {
    this.home = Principal.fromText(home)
  }

  async journal(): Promise<ControlJournal | null> {
    const value = await this.crypto.call('controlGet', 'operation')
    return value ? (JSON.parse(value) as ControlJournal) : null
  }
  private async save(journal: ControlJournal) {
    await this.crypto.call('controlPut', 'operation', JSON.stringify(journal))
  }
  async pending() {
    const journal = await this.journal()
    return journal && ['prepared', 'unknown'].includes(journal.stage) ? journal : null
  }
  async connectedAccount() {
    const result = await this.user.my_account()
    return result.length ? xidText(Uint8Array.from(result[0]!)) : null
  }
  /** The login-gated secret that unlocks this device's local store. */
  async unlockSecret(account: string) {
    const secret = Uint8Array.from(
      controlResult(await this.user.unlock_secret(xidBytes(account), unhex(this.meta.deviceId)))
    )
    ensure(secret.length === 32, 'INTEGRITY_FAILED')
    return secret
  }
  async refresh(account: string) {
    const raw = xidBytes(account)
    const [infoResult, batchResult] = await Promise.all([
      this.user.get_account(raw),
      this.user.security_snapshot_batch([raw])
    ])
    const info = controlResult(infoResult)
    const ownDevice = info.devices.find(([key]) => same(key, unhex(this.meta.deviceId)))?.[1]
    if (ownDevice)
      ensure(
        same(ownDevice.input.signing_pub, unb64(this.meta.signingPublic)) &&
          same(ownDevice.input.hpke_pub, unb64(this.meta.hpkePublic)),
        'INTEGRITY_FAILED'
      )
    ensure(
      same(info.account_id, raw) && info.home_user.toText() === this.home.toText(),
      'INTEGRITY_FAILED'
    )
    // The full device map and every security-sensitive public field come from
    // the same certified version. A race rejects rather than mixing snapshots.
    const bundleResult = controlResult(await this.user.get_device_bundle(raw))
    ensure(
      equal(canonical(accountValue(info.devices)), canonical(accountValue(bundleResult[1]))),
      'POLICY_STALE'
    )
    const verified = await verifyCloudSecurity(
      controlResult(batchResult),
      bundleResult,
      this.agent,
      {
        accountId: account,
        homeUser: this.home.toText(),
        issuer: info.issuer
      },
      Date.now()
    )
    const snapshot = verified.snapshot
    if (this.meta.account?.id === account) {
      ensure(
        (info.current_root[0]?.generation ?? 0n) >= BigInt(this.meta.rootGeneration),
        'POLICY_STALE'
      )
      if (
        info.current_root[0]?.generation === BigInt(this.meta.rootGeneration) &&
        this.meta.account.rootDigest
      )
        ensure(
          hex(Uint8Array.from(info.current_root[0].bundle_digest)) ===
            this.meta.account.rootDigest,
          'INTEGRITY_FAILED'
        )
    }
    ensure(
      BigInt(snapshot.account_version as number | bigint) === info.account_version &&
        BigInt(snapshot.security_epoch as number | bigint) === info.security_epoch &&
        snapshot.vault_write_state === Object.keys(info.vault_write_state)[0] &&
        snapshot.home_cose instanceof Uint8Array &&
        equal(snapshot.home_cose, info.home_cose.toUint8Array()) &&
        BigInt(snapshot.recovery_delay_ms as number | bigint) === info.recovery_delay_ms &&
        BigInt(snapshot.content_root_generation as number | bigint) ===
          (info.current_root[0]?.generation ?? 0n) &&
        equal(
          canonical(snapshot.content_root_digest),
          canonical(
            info.current_root[0] ? Uint8Array.from(info.current_root[0].bundle_digest) : null
          )
        ) &&
        equal(
          canonical(snapshot.pending_recovery_digest),
          canonical(
            info.pending_recovery[0] ? pendingRecoveryDigest(info.pending_recovery[0]) : null
          )
        ),
      'INTEGRITY_FAILED'
    )
    const highKey = `observed:${this.home.toText()}:${account}`
    const saved = await this.crypto.call('controlGet', highKey)
    if (saved) {
      const previous = JSON.parse(saved)
      ensure(
        info.account_version >= BigInt(previous.version) &&
          info.security_epoch >= BigInt(previous.epoch) &&
          (info.current_root[0]?.generation ?? 0n) >= BigInt(previous.root),
        'POLICY_STALE',
        '拒绝比本机已验证状态更旧的账户响应。'
      )
    }
    await this.crypto.call(
      'controlPut',
      highKey,
      JSON.stringify({
        version: String(info.account_version),
        epoch: String(info.security_epoch),
        root: String(info.current_root[0]?.generation ?? 0n)
      })
    )
    return {
      info,
      verified,
      device: info.devices.find(([key]) => same(key, unhex(this.meta.deviceId)))?.[1]
    }
  }
  private async dispatch(journal: ControlJournal) {
    ensure(
      journal.home === this.home.toText() && journal.caller === this.caller.toText(),
      'AuthRequired',
      '请使用原请求的认证身份继续。'
    )
    // Save unknown before the call: termination can occur after server commit.
    journal = { ...journal, stage: 'unknown' }
    await this.save(journal)
    const args = decodeControl(journal.method, journal.args)
    let response: { Ok: unknown } | { Err: unknown }
    try {
      const call = this.user[journal.method] as (
        ...args: unknown[]
      ) => Promise<{ Ok: unknown } | { Err: unknown }>
      response = await call(...args)
    } catch {
      throw new DmsgError('EXECUTION_UNKNOWN', '响应尚未确认。请重新连接后查询原操作。')
    }
    if ('Err' in response) {
      const code = Object.keys(response.Err as object)[0]
      // After a prior dispatch an expired/pruned receipt is not evidence that
      // the old request never committed. Preserve the journal for inspection.
      if (
        !['ResultExpired', 'Expired'].includes(code) ||
        (code === 'Expired' && journal.method === 'create_account')
      )
        await this.save({ ...journal, stage: 'rejected', error: code })
      return controlResult(response)
    }
    if (journal.method === 'mutate_account') {
      const receipt = response.Ok as { id: Uint8Array; digest: Uint8Array }
      ensure(
        same(receipt.id, unhex(journal.requestId)) &&
          same(receipt.digest, accountOperationDigest(args[0] as AccountMutation)),
        'INTEGRITY_FAILED'
      )
    }
    if (journal.method === 'create_account')
      journal.account = xidText(Uint8Array.from(response.Ok as Uint8Array))
    await this.save({ ...journal, stage: 'confirmed' })
    return response.Ok
  }
  async resume() {
    const journal = await this.pending()
    ensure(journal, 'NOT_FOUND')
    ensure(
      journal.home === this.home.toText() && journal.caller === this.caller.toText(),
      'AuthRequired'
    )
    if (journal.method === 'create_account') {
      const account = await this.connectedAccount()
      if (account) {
        // Finding an account does not prove this device won account creation.
        await this.refresh(account)
        await this.save({ ...journal, account, stage: 'confirmed' })
        return account
      }
    } else if (journal.method === 'mutate_account') {
      const result = await this.user.get_operation(
        xidBytes(journal.account!),
        unhex(journal.requestId)
      )
      if ('Ok' in result) {
        const request = decodeControl(journal.method, journal.args)[0] as AccountMutation
        ensure(
          same(result.Ok.id, request.approval.request_id) &&
            same(result.Ok.digest, accountOperationDigest(request)),
          'INTEGRITY_FAILED'
        )
        await this.refresh(journal.account!)
        await this.save({ ...journal, stage: 'confirmed' })
        return journal.account
      }
      ensure('ResultExpired' in result.Err, 'UNAVAILABLE')
      const request = decodeControl(journal.method, journal.args)[0] as AccountMutation
      const { info, device, verified } = await this.refresh(journal.account!)
      if (
        BigInt(verified.certifiedAt) >= request.approval.expires_at &&
        info.account_version === request.expected_version &&
        device?.next_sequence === request.approval.sequence
      ) {
        await this.save({ ...journal, stage: 'rejected', error: 'Expired' })
        return journal.account
      }
    }
    await this.dispatch(journal)
    return (await this.journal())!.account
  }
  /** Create this login's account. A home with an admission key needs a
   * ticket from `admit`, which asks the cloud service for one. */
  async create(admit?: (home: Uint8Array, principal: Uint8Array) => Promise<AdmissionTicket>) {
    ensure(!(await this.pending()), 'Pending', '先查询或继续上一个操作。')
    const existing = await this.connectedAccount()
    if (existing) {
      await this.refresh(existing)
      return existing
    }
    const { admission_key } = await this.user.user_config()
    let admission: [] | [AdmissionTicket] = []
    if (admission_key.length) {
      ensure(admit, 'UNAVAILABLE', '此账户服务需要云端准入票据，请先配置云端服务。')
      admission = [await admit(this.home.toUint8Array(), this.caller.toUint8Array())]
    }
    const input: CreateAccount = {
      device: deviceInput(this.meta),
      op_id: unhex(id()),
      expires_at: BigInt(Date.now() + 270000),
      proof: new Uint8Array(),
      admission
    }
    input.proof = await this.crypto.call(
      'deviceSign',
      createAccountMessage(this.home, this.caller, input)
    )
    const journal: ControlJournal = {
      version: 1,
      home: this.home.toText(),
      caller: this.caller.toText(),
      account: null,
      method: 'create_account',
      args: encodeControl('create_account', [input]),
      requestId: hex(Uint8Array.from(input.op_id)),
      stage: 'prepared'
    }
    await this.save(journal)
    await this.dispatch(journal)
    const account = (await this.journal())!.account!
    await this.refresh(account)
    return account
  }
  /** Disputing a recovery needs any active device; everything else needs an
   * administrator with root management. */
  async mutate(
    account: string,
    command:
      AccountCommand | ((info: AccountInfo, requestId: Uint8Array) => Promise<AccountCommand>),
    fixed?: { requestId: string; version: bigint }
  ) {
    ensure(!(await this.pending()), 'Pending', '先查询或继续上一个操作。')
    const { info, device } = await this.refresh(account)
    ensure(
      device &&
        !device.revoked_at.length &&
        same(device.input.signing_pub, unb64(this.meta.signingPublic)),
      'DeviceNotApproved'
    )
    if (fixed)
      ensure(
        info.account_version === fixed.version,
        'VERSION_CONFLICT',
        '账户已变化，请新设备重新生成批准请求。'
      )
    const requestId = unhex(fixed?.requestId ?? id())
    const request: AccountMutation = {
      account_id: xidBytes(account),
      expected_version: info.account_version,
      command: typeof command === 'function' ? await command(info, requestId) : command,
      approval: {
        device_id: unhex(this.meta.deviceId),
        security_epoch: info.security_epoch,
        sequence: device.next_sequence,
        request_id: requestId,
        expires_at: BigInt(Date.now() + 270000),
        signature: new Uint8Array()
      }
    }
    if ('AuthorizeHandle' in request.command)
      request.approval.expires_at = BigInt(Date.now() + 55000)
    ensure(
      'DisputeRecovery' in request.command ||
        ('Administrator' in device.input.role &&
          device.input.capabilities.some((c) => 'RootManage' in c)),
      'Forbidden'
    )
    request.approval.signature = await this.crypto.call(
      'deviceSign',
      accountApprovalMessage(this.home, request)
    )
    const journal: ControlJournal = {
      version: 1,
      home: this.home.toText(),
      caller: this.caller.toText(),
      account,
      method: 'mutate_account',
      args: encodeControl('mutate_account', [request]),
      requestId: hex(requestId),
      stage: 'prepared'
    }
    await this.save(journal)
    await this.dispatch(journal)
    return this.refresh(account)
  }

  async pairing(account: string, role: 'Administrator' | 'Member') {
    const { info } = await this.refresh(account),
      requestId = unhex(id()),
      device = deviceInput(this.meta, role)
    const proof = await this.crypto.call(
      'deviceSign',
      digest('dmsg/add-device/v1', [
        this.home.toUint8Array(),
        xidBytes(account),
        accountValue(device),
        info.account_version,
        requestId
      ])
    )
    const request: AccountMutation = {
      account_id: xidBytes(account),
      expected_version: info.account_version,
      command: { AddDevice: { device, proof } },
      approval: {
        device_id: unhex(this.meta.deviceId),
        security_epoch: info.security_epoch,
        sequence: 0n,
        request_id: requestId,
        expires_at: BigInt(Date.now() + 270000),
        signature: new Uint8Array()
      }
    }
    return JSON.stringify({
      format: 'dmsg-device-request/1',
      home: this.home.toText(),
      request: encodeControl('mutate_account', [request])
    })
  }
  inspectPairing(text: string) {
    ensure(text.length <= 16000, 'INVALID_INPUT')
    const packet = JSON.parse(text)
    ensure(
      packet.format === 'dmsg-device-request/1' && packet.home === this.home.toText(),
      'INTEGRITY_FAILED'
    )
    const request = decodeControl('mutate_account', packet.request)[0] as AccountMutation
    ensure(
      'AddDevice' in request.command &&
        request.approval.expires_at > BigInt(Date.now()) &&
        request.approval.expires_at <= BigInt(Date.now() + 300000),
      'EXPIRED'
    )
    const input = request.command.AddDevice
    ensure(
      ed25519.verify(
        Uint8Array.from(input.proof),
        digest('dmsg/add-device/v1', [
          this.home.toUint8Array(),
          Uint8Array.from(request.account_id),
          accountValue(input.device),
          request.expected_version,
          Uint8Array.from(request.approval.request_id)
        ]),
        Uint8Array.from(input.device.signing_pub),
        { zip215: false }
      ),
      'INTEGRITY_FAILED'
    )
    return request
  }
  async approvePairing(account: string, text: string) {
    const request = this.inspectPairing(text)
    ensure(same(request.account_id, xidBytes(account)), 'INTEGRITY_FAILED')
    return this.mutate(account, request.command, {
      requestId: hex(Uint8Array.from(request.approval.request_id)),
      version: request.expected_version
    })
  }

  private async submit(
    method: ControlMethod,
    account: string,
    args: unknown[],
    requestId: string
  ) {
    ensure(!(await this.pending()), 'Pending')
    const journal: ControlJournal = {
      version: 1,
      home: this.home.toText(),
      caller: this.caller.toText(),
      account,
      method,
      args: encodeControl(method, args),
      requestId,
      stage: 'prepared'
    }
    await this.save(journal)
    return this.dispatch(journal)
  }
  private bindingKey(account: string) {
    return `binding:${account}`
  }
  /** This login's request for an administrator device to bind it. Nothing
   * reaches the home until the login accepts the approval. */
  async bindingRequest(account: string) {
    const nonce = id(),
      expires = Date.now() + 300000
    await this.crypto.call(
      'controlPut',
      this.bindingKey(account),
      JSON.stringify({ principal: this.caller.toText(), nonce })
    )
    return JSON.stringify({
      format: 'dmsg-auth-request/1',
      home: this.home.toText(),
      account,
      principal: this.caller.toText(),
      nonce,
      expires
    })
  }
  inspectBinding(text: string) {
    ensure(text.length <= 4000, 'INVALID_INPUT')
    const packet = JSON.parse(text)
    ensure(
      packet.format === 'dmsg-auth-request/1' &&
        packet.home === this.home.toText() &&
        packet.expires > Date.now() &&
        packet.expires <= Date.now() + 300000,
      'EXPIRED'
    )
    xidBytes(packet.account)
    Principal.fromText(packet.principal)
    ensure(unhex(packet.nonce).length === 32, 'INVALID_INPUT')
    return packet as { account: string; principal: string; nonce: string; expires: number }
  }
  /** Accept the binding an administrator approved for this login's request;
   * the approval lapses ten minutes after it was given. */
  async acceptBinding(account: string) {
    const saved = await this.crypto.call('controlGet', this.bindingKey(account))
    const request = saved ? (JSON.parse(saved) as { principal: string; nonce: string }) : null
    ensure(
      request && request.principal === this.caller.toText(),
      'NOT_FOUND',
      '请先用当前登录身份生成认证绑定请求。'
    )
    await this.submit(
      'accept_auth_binding',
      account,
      [xidBytes(account), unhex(request.nonce)],
      request.nonce
    )
    return this.refresh(account)
  }
  async approveBinding(account: string, text: string) {
    const request = this.inspectBinding(text)
    ensure(request.account === account, 'INTEGRITY_FAILED')
    return this.mutate(account, {
      BindAuth: {
        principal: Principal.fromText(request.principal),
        nonce: unhex(request.nonce)
      }
    })
  }
  async snapshot(account: string) {
    const raw = xidBytes(account),
      [a, b] = await Promise.all([
        this.user.security_snapshot_batch([raw]),
        this.user.get_device_bundle(raw)
      ])
    const bundle = controlResult(b)
    return verifyCloudSecurity(
      controlResult(a),
      bundle,
      this.agent,
      { accountId: account, homeUser: this.home.toText(), issuer: bundle[0].issuer },
      Date.now()
    )
  }
  async recoveryStatus(account: string) {
    const [verified, result] = await Promise.all([
      this.snapshot(account),
      this.user.get_recovery_request(xidBytes(account))
    ])
    const pending = controlResult(result)[0] ?? null
    ensure(
      equal(
        canonical(verified.snapshot.pending_recovery_digest),
        canonical(pending ? pendingRecoveryDigest(pending) : null)
      ),
      'POLICY_STALE'
    )
    return { pending, verified }
  }
  /** Begin a login-authorized takeover onto this device. The bound login is
   * the authority; the device only proves possession of its keys. */
  async requestRecovery(account: string) {
    const { snapshot } = await this.snapshot(account)
    const delay = Number(snapshot.recovery_delay_ms as number | bigint)
    const request: RecoveryRequest = {
      op_id: unhex(id()),
      new_auth: this.caller,
      device: deviceInput(this.meta),
      // At most 14 days past the home's clock, so leave a skew margin.
      expires_at: BigInt(Date.now() + delay + 7 * 86400000 - 600000)
    }
    const proof = await this.crypto.call(
      'deviceSign',
      recoveryDeviceMessage(this.home, account, request)
    )
    await this.submit(
      'request_recovery',
      account,
      [xidBytes(account), request, proof],
      hex(Uint8Array.from(request.op_id))
    )
    return this.recoveryStatus(account)
  }
  async disputeRecovery(account: string) {
    const { pending } = await this.recoveryStatus(account)
    ensure(pending, 'NOT_FOUND')
    await this.mutate(account, { DisputeRecovery: { op_id: pending.request.op_id } })
    return this.recoveryStatus(account)
  }
  async completeRecovery(account: string) {
    const { pending } = await this.recoveryStatus(account)
    ensure(
      pending &&
        pending.request.new_auth.toText() === this.caller.toText() &&
        same(pending.request.device.device_id, unhex(this.meta.deviceId)),
      'AuthRequired'
    )
    ensure(Date.now() >= Number(pending.execute_after), 'LOCKED', '恢复等待期尚未结束。')
    await this.submit(
      'complete_recovery',
      account,
      [xidBytes(account), pending.request.op_id],
      hex(Uint8Array.from(pending.request.op_id))
    )
    return this.refresh(account)
  }
  setRecoveryDelay(account: string, days: number) {
    ensure(Number.isInteger(days) && days >= 1 && days <= 7, 'INVALID_INPUT')
    return this.mutate(account, { SetRecoveryDelay: { delay_ms: BigInt(days * 86400000) } })
  }
}
