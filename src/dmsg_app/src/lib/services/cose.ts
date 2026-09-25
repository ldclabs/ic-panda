import { encodeControl, decodeControl } from '../protocol/account'
import { actionToCandid, actionFromCandid } from '../protocol/app-action'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE as UserService,
  Approval,
  DeriveRootRequest,
  ExecutionResult,
  Statement,
  RootTarget,
  SignRequest,
  AppActionSignRequest,
  SigningAlgorithm
} from '../canisters/generated/user'
import type { _SERVICE as CoseService, KeySelector } from '../canisters/generated/cose'
import { digest, equal, hex } from '../protocol/codec'
import { DmsgError, ensure } from '../errors'
import { controlResult } from './account'
import {
  statementBytes,
  statementPurpose,
  verifyDocumentArtifact,
  type Algorithm,
  type DocumentStatement
} from '../protocol/statements'
export type { Algorithm } from '../protocol/statements'

export class ExecutionRejected extends DmsgError {}

export interface SigningKeyReference {
  algorithm: Algorithm
  kid: Uint8Array
  publicKeyFingerprint: Uint8Array
}

/** Obtain device/epoch/sequence from the authenticated account before preparing.
 * The origin must come from the extension's checked browser message, not a
 * third-party request field. A device signature cannot prove browser provenance.
 */
export interface ExecutionContext {
  homeUser: Principal
  accountId: Uint8Array
  issuer: string
  deviceId: Uint8Array
  securityEpoch: bigint
  sequence: bigint
  expiresAt: bigint
  maxCycles: bigint
}
type Operation =
  | { kind: 'sign'; request: SignRequest }
  | { kind: 'sign_app_action'; request: AppActionSignRequest }
  | { kind: 'derive_root'; request: DeriveRootRequest }
function cloneOperation(operation: Operation): Operation {
  // structuredClone strips Principal's prototype. Clone through the exact public
  // Candid schema so receiver identity survives approval, journaling and retry.
  return {
    kind: operation.kind,
    request: decodeControl(
      operation.kind,
      encodeControl(operation.kind, [operation.request])
    )[0]
  } as Operation
}
type UserExecution = Pick<
  UserService,
  'sign' | 'sign_app_action' | 'derive_root' | 'get_execution' | 'reconcile_execution'
>
export type DeviceSigner = (approvalDigest: Uint8Array) => Promise<Uint8Array>

function fixed(value: Uint8Array, size: number) {
  ensure(
    value instanceof Uint8Array && value.length === size,
    'INVALID_INPUT',
    `需要 ${size} 字节。`
  )
  ensure(
    value.some((byte) => byte !== 0),
    'INVALID_INPUT',
    '标识或公钥不能全为零。'
  )
  return Uint8Array.from(value)
}
function uint64(value: bigint) {
  ensure(
    typeof value === 'bigint' && value >= 0n && value <= 0xffffffffffffffffn,
    'INVALID_INPUT'
  )
  return value
}
function approval(context: ExecutionContext, now: number): Approval {
  const account = fixed(context.accountId, 12),
    device = fixed(context.deviceId, 32)
  const epoch = uint64(context.securityEpoch),
    sequence = uint64(context.sequence)
  const expiresAt = uint64(context.expiresAt)
  ensure(
    Number.isSafeInteger(now) && expiresAt > BigInt(now) && expiresAt <= BigInt(now) + 300000n,
    'EXPIRED',
    '批准有效期必须在未来 5 分钟内。'
  )
  ensure(context.maxCycles > 0n && context.maxCycles <= 100000000000n, 'QUOTA_EXCEEDED')
  ensure(
    !context.homeUser.isAnonymous() && context.homeUser.toUint8Array().length > 0,
    'INVALID_INPUT'
  )
  return {
    device_id: device,
    security_epoch: epoch,
    sequence,
    request_id: digest('dmsg/execution-request/v2', [account, epoch, device, sequence]),
    expires_at: expiresAt,
    signature: new Uint8Array()
  }
}
function toStatement(input: DocumentStatement): Statement {
  let content: Statement['content']
  switch (input.content.kind) {
    case 'app_action':
      content = { AppAction: actionToCandid(input.content.action) }
      break
    case 'text':
      content = { Text: input.content.text }
      break
    case 'digest':
      content = {
        Digest: {
          sha256: input.content.sha256,
          content_type:
            input.content.contentType === undefined ? [] : [input.content.contentType],
          location: input.content.location === undefined ? [] : [input.content.location]
        }
      }
      break
    case 'file_statement':
      content = {
        FileStatement: {
          text: input.content.text,
          sha256: input.content.sha256,
          content_type:
            input.content.contentType === undefined ? [] : [input.content.contentType],
          location: input.content.location === undefined ? [] : [input.content.location]
        }
      }
      break
  }
  return {
    issuer: input.issuer,
    subject: input.subject === undefined ? [] : [input.subject],
    issued_at: input.issuedAt === undefined ? [] : [input.issuedAt],
    content
  }
}
function fromStatement(s: Statement): DocumentStatement {
  return {
    issuer: s.issuer,
    subject: s.subject[0],
    issuedAt: s.issued_at[0],
    content:
      'AppAction' in s.content
        ? { kind: 'app_action', action: actionFromCandid(s.content.AppAction) }
        : 'Text' in s.content
          ? { kind: 'text', text: s.content.Text }
          : 'FileStatement' in s.content
            ? {
                kind: 'file_statement',
                text: s.content.FileStatement.text,
                sha256: Uint8Array.from(s.content.FileStatement.sha256),
                contentType: s.content.FileStatement.content_type[0],
                location: s.content.FileStatement.location[0]
              }
            : {
                kind: 'digest',
                sha256: Uint8Array.from(s.content.Digest.sha256),
                contentType: s.content.Digest.content_type[0],
                location: s.content.Digest.location[0]
              }
  }
}
export function signingRequest(request: SignRequest | AppActionSignRequest): SignRequest {
  return 'action' in request
    ? {
        ...request,
        origin: request.action.origin,
        statement: {
          issuer: request.issuer,
          subject: [],
          issued_at: [],
          content: { AppAction: request.action }
        }
      }
    : request
}
export function signBytes(input: SignRequest | AppActionSignRequest) {
  const request = signingRequest(input)
  return statementBytes(
    fromStatement(request.statement),
    Object.keys(request.key.algorithm)[0] as Algorithm,
    Uint8Array.from(request.key.kid)
  ).toBeSigned
}
function executionKind(operation: Operation): unknown {
  if (operation.kind !== 'derive_root') {
    const sign = signingRequest(operation.request)
    return {
      Sign: {
        key: {
          purpose: statementPurpose(fromStatement(sign.statement).content),
          algorithm: Object.keys(sign.key.algorithm)[0],
          generation: 1n
        },
        to_be_signed: signBytes(sign),
        public_key_fingerprint: Uint8Array.from(sign.key.public_key_fingerprint),
        origin: sign.origin
      }
    }
  }
  const root = operation.request
  const target = 'Current' in root.target ? root.target.Current : root.target.Candidate
  return {
    Derive: {
      generation: target.generation,
      root_op_id:
        'Candidate' in root.target ? Uint8Array.from(root.target.Candidate.op_id) : null,
      transport_key: Uint8Array.from(root.transport_public_key)
    }
  }
}
/** A snapshot of exactly what will be approved. Returned views are copies.
 * Persist the signed operation (and the root transport-secret reference) with
 * `onApproved` before dispatch when integrating an encrypted outbox.
 */
export class PreparedExecution {
  private readonly operation: Operation
  private readonly message: Uint8Array
  private submission?: Promise<ExecutionResult>

  constructor(operation: Operation, homeUser: Principal) {
    this.operation = cloneOperation(operation)
    const request = this.operation.request,
      a = request.approval
    this.message = digest('dmsg/device-approval/v2', [
      homeUser.toUint8Array(),
      Uint8Array.from(request.account_id),
      'dmsg/execute/v3',
      Uint8Array.from(a.device_id),
      a.security_epoch,
      a.sequence,
      Uint8Array.from(a.request_id),
      a.expires_at,
      digest('dmsg/execute/v3', [executionKind(this.operation), request.max_cycles])
    ])
  }
  get requestId() {
    return hex(Uint8Array.from(this.operation.request.approval.request_id))
  }
  get approvalMessage() {
    return Uint8Array.from(this.message)
  }
  get review() {
    return cloneOperation(this.operation)
  }
  get toBeSigned() {
    return this.operation.kind !== 'derive_root' ? signBytes(this.operation.request) : null
  }
  approveAndExecute(
    user: Pick<UserExecution, 'sign' | 'sign_app_action' | 'derive_root'>,
    signer: DeviceSigner,
    onApproved: (operation: Operation) => Promise<void> = async () => {}
  ): Promise<ExecutionResult> {
    this.submission ??= this.submit(user, signer, onApproved)
    return this.submission
  }
  private async submit(
    user: Pick<UserExecution, 'sign' | 'sign_app_action' | 'derive_root'>,
    signer: DeviceSigner,
    onApproved: (operation: Operation) => Promise<void>
  ) {
    const signed = cloneOperation(this.operation)
    signed.request.approval.signature = fixed(await signer(this.approvalMessage), 64)
    await onApproved(cloneOperation(signed))
    let response: Awaited<ReturnType<UserService['sign']>>
    try {
      response =
        signed.kind === 'sign'
          ? await user.sign(signed.request)
          : signed.kind === 'sign_app_action'
            ? await user.sign_app_action(signed.request)
            : await user.derive_root(signed.request)
    } catch {
      throw new DmsgError(
        'EXECUTION_UNKNOWN',
        `执行结果尚未确认，请按请求 ${this.requestId} 对账。`
      )
    }
    if ('Err' in response) {
      try {
        controlResult(response)
      } catch (error) {
        if (error instanceof DmsgError) throw new ExecutionRejected(error.code, error.message)
        throw error
      }
    }
    const result = controlResult(response)
    ensure(
      equal(
        Uint8Array.from(result.request_id),
        Uint8Array.from(signed.request.approval.request_id)
      ),
      'INTEGRITY_FAILED',
      '响应与原请求不一致。'
    )
    if (signed.kind !== 'derive_root' && 'Completed' in result.outcome) {
      const output = result.outcome.Completed
      ensure('Signature' in output, 'INTEGRITY_FAILED')
      const checked = verifyDocumentArtifact(output.Signature.artifact)
      ensure(
        equal(checked.toBeSigned, signBytes(signed.request)) &&
          equal(
            checked.keyFingerprint,
            Uint8Array.from(signed.request.key.public_key_fingerprint)
          ),
        'INTEGRITY_FAILED',
        '签名结果与批准的内容或密钥不符。'
      )
    }
    return result
  }
}

export function prepareSign(
  context: ExecutionContext,
  content: { origin: string; key: SigningKeyReference; statement: DocumentStatement },
  now = Date.now()
) {
  const { key, statement } = content
  ensure(statement.issuer === context.issuer, 'FORBIDDEN', '签署者必须与已认证账户一致。')
  statementBytes(statement, key.algorithm, key.kid)
  const origin = new URL(content.origin)
  ensure(
    content.origin.length <= 256 &&
      ((statement.content.kind === 'app_action' &&
        statement.content.action.origin === content.origin) ||
        (origin.protocol === 'https:' && origin.origin === content.origin) ||
        /^chrome-extension:\/\/[a-p]{32}$/.test(content.origin)),
    'INVALID_INPUT',
    '需要规范的应用 origin。'
  )
  if (statement.content.kind === 'app_action')
    return new PreparedExecution(
      {
        kind: 'sign_app_action',
        request: {
          account_id: fixed(context.accountId, 12),
          issuer: context.issuer,
          action: actionToCandid(statement.content.action),
          key: {
            algorithm: { [key.algorithm]: null } as SigningAlgorithm,
            kid: Uint8Array.from(key.kid),
            public_key_fingerprint: fixed(key.publicKeyFingerprint, 32)
          },
          max_cycles: context.maxCycles,
          approval: approval(context, now)
        }
      },
      context.homeUser
    )
  return new PreparedExecution(
    {
      kind: 'sign',
      request: {
        account_id: fixed(context.accountId, 12),
        key: {
          algorithm: { [key.algorithm]: null } as SigningAlgorithm,
          kid: Uint8Array.from(key.kid),
          public_key_fingerprint: fixed(key.publicKeyFingerprint, 32)
        },
        statement: toStatement(statement),
        origin: content.origin,
        max_cycles: context.maxCycles,
        approval: approval(context, now)
      }
    },
    context.homeUser
  )
}

export function prepareRootDerivation(
  context: ExecutionContext,
  target:
    | { kind: 'current'; generation: bigint }
    | { kind: 'candidate'; generation: bigint; opId: Uint8Array },
  transportPublicKey: Uint8Array,
  now = Date.now()
) {
  ensure(uint64(target.generation) > 0n, 'INVALID_INPUT')
  ensure(target.kind === 'current' || target.kind === 'candidate', 'INVALID_INPUT')
  const root: RootTarget =
    target.kind === 'current'
      ? { Current: { generation: target.generation } }
      : { Candidate: { generation: target.generation, op_id: fixed(target.opId, 32) } }
  return new PreparedExecution(
    {
      kind: 'derive_root',
      request: {
        account_id: fixed(context.accountId, 12),
        target: root,
        transport_public_key: fixed(transportPublicKey, 48),
        max_cycles: context.maxCycles,
        approval: approval(context, now)
      }
    },
    context.homeUser
  )
}

export async function signingKey(
  cose: Pick<CoseService, 'public_key'>,
  accountId: Uint8Array,
  purpose: 'statement' | 'file_attestation' | 'app_action',
  algorithm: Algorithm = 'Ed25519'
) {
  ensure(['Ed25519', 'EcdsaSecp256k1'].includes(algorithm), 'UNSUPPORTED_PROTOCOL')
  const key: KeySelector = {
    Signing: {
      purpose:
        purpose === 'app_action'
          ? { AppAction: null }
          : purpose === 'statement'
            ? { Statement: null }
            : { FileAttestation: null },
      algorithm: { [algorithm]: null } as SigningAlgorithm
    }
  }
  return controlResult(await cose.public_key(fixed(accountId, 12), key))
}
/** Reads the stored result of an approved execution, reconciling one still in
 * flight. Null means no execution is stored under this request ID. */
export async function recordedExecution(
  user: Pick<UserExecution, 'get_execution' | 'reconcile_execution'>,
  accountId: Uint8Array,
  requestId: Uint8Array
): Promise<ExecutionResult | null> {
  const account = fixed(accountId, 12),
    request = fixed(requestId, 32),
    response = await user.get_execution(account, request)
  if ('Err' in response) {
    ensure('ResultExpired' in response.Err, 'EXECUTION_UNKNOWN')
    return null
  }
  const outcome = response.Ok.outcome
  return 'Completed' in outcome || 'Failed' in outcome || 'ResultExpired' in outcome
    ? response.Ok
    : controlResult(await user.reconcile_execution(account, request))
}
