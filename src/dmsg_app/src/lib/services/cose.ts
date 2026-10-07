import { attestApprovalMessage, decodeControl, deriveApprovalMessage, encodeControl } from '../protocol/account'
import { actionToCandid, actionFromCandid } from '../protocol/app-action'
import { Principal } from '@icp-sdk/core/principal'
import type {
  _SERVICE as UserService,
  Approval,
  AppActionAttestRequest,
  AttestRequest,
  DeriveRootRequest,
  ExecutionResult,
  SignedArtifact,
  Statement
} from '../canisters/generated/user'
import { sha256 } from '@noble/hashes/sha2.js'
import { canonical, digest, equal, hex } from '../protocol/codec'
import { DmsgError, ensure } from '../errors'
import { controlResult } from './account'
import { statementBytes, verifyDocumentArtifact, type DocumentStatement } from '../protocol/statements'

export class ExecutionRejected extends DmsgError {}

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
}
export type AttestOperation =
  | { kind: 'attest'; request: AttestRequest }
  | { kind: 'attest_app_action'; request: AppActionAttestRequest }
function cloneOperation<T extends AttestOperation>(operation: T): T {
  // structuredClone strips Principal's prototype. Clone through the exact public
  // Candid schema so receiver identity survives approval, journaling and retry.
  return {
    kind: operation.kind,
    request: decodeControl(operation.kind, encodeControl(operation.kind, [operation.request]))[0]
  } as T
}
export type DeviceSigner = (message: Uint8Array) => Promise<Uint8Array>

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
/** RFC 9679 thumbprint of a raw Ed25519 public key; the artifact kid. */
export function deviceKeyThumbprint(publicKey: Uint8Array) {
  ensure(publicKey.length === 32, 'INVALID_INPUT')
  return sha256(
    canonical(
      new Map<number, unknown>([
        [1, 1],
        [-1, 6],
        [-2, Uint8Array.from(publicKey)]
      ])
    )
  )
}
export function toStatement(input: DocumentStatement): Statement {
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
export function fromStatement(s: Statement): DocumentStatement {
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
/** The statement and checked origin an attestation request binds. */
export function attestView(request: AttestRequest | AppActionAttestRequest) {
  return 'action' in request
    ? {
        statement: {
          issuer: request.issuer,
          subject: [] as [],
          issued_at: [] as [],
          content: { AppAction: request.action }
        } satisfies Statement,
        origin: request.action.origin
      }
    : { statement: request.statement, origin: request.origin }
}
/** The exact Sig_structure a device signs for an attestation request. */
export function signBytes(
  request: AttestRequest | AppActionAttestRequest,
  kid: Uint8Array
) {
  return statementBytes(fromStatement(attestView(request).statement), kid).toBeSigned
}
/** A frozen attestation: the device signs the Sig_structure, then approves
 * the statement, origin and that signature together. */
export class PreparedAttestation {
  private readonly operation: AttestOperation
  private readonly thumbprint: Uint8Array
  private readonly signingBytes: Uint8Array
  private submission?: Promise<SignedArtifact>

  constructor(
    operation: AttestOperation,
    readonly homeUser: Principal,
    devicePublicKey: Uint8Array
  ) {
    this.operation = cloneOperation(operation)
    this.thumbprint = deviceKeyThumbprint(devicePublicKey)
    this.signingBytes = signBytes(this.operation.request, this.thumbprint)
  }
  get requestId() {
    return hex(Uint8Array.from(this.operation.request.approval.request_id))
  }
  /** RFC 9679 thumbprint of the device key; the artifact kid. */
  get kid() {
    return Uint8Array.from(this.thumbprint)
  }
  /** The exact Sig_structure the device signs. Returned views are copies. */
  get toBeSigned() {
    return Uint8Array.from(this.signingBytes)
  }
  get review() {
    return cloneOperation(this.operation)
  }
  /** The approval digest of the signed request, for journals and vectors. */
  approvalMessage(signature: Uint8Array): Uint8Array {
    const { statement, origin } = attestView(this.operation.request)
    return attestApprovalMessage(this.homeUser, {
      account_id: this.operation.request.account_id,
      statement,
      origin,
      signature,
      approval: this.operation.request.approval
    })
  }
  approveAndExecute(
    user: Pick<UserService, 'attest' | 'attest_app_action'>,
    signer: DeviceSigner,
    onApproved: (operation: AttestOperation) => Promise<void> = async () => {}
  ): Promise<SignedArtifact> {
    this.submission ??= this.submit(user, signer, onApproved)
    return this.submission
  }
  private async submit(
    user: Pick<UserService, 'attest' | 'attest_app_action'>,
    signer: DeviceSigner,
    onApproved: (operation: AttestOperation) => Promise<void>
  ) {
    const signed = cloneOperation(this.operation)
    signed.request.signature = fixed(await signer(this.toBeSigned), 64)
    signed.request.approval.signature = fixed(
      await signer(this.approvalMessage(Uint8Array.from(signed.request.signature))),
      64
    )
    await onApproved(cloneOperation(signed))
    return submitAttestation(user, signed, this.signingBytes, this.thumbprint)
  }
}
/** Submit a signed attestation request and check the returned artifact. */
export async function submitAttestation(
  user: Pick<UserService, 'attest' | 'attest_app_action'>,
  signed: AttestOperation,
  toBeSigned: Uint8Array,
  kid: Uint8Array
) {
  let response: Awaited<ReturnType<UserService['attest']>>
  try {
    response =
      signed.kind === 'attest'
        ? await user.attest(signed.request)
        : await user.attest_app_action(signed.request)
  } catch {
    throw new DmsgError(
      'EXECUTION_UNKNOWN',
      `执行结果尚未确认，请按请求 ${hex(Uint8Array.from(signed.request.approval.request_id))} 对账。`
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
  const artifact = controlResult(response)
  checkArtifact(artifact, toBeSigned, kid)
  return artifact
}
export function checkArtifact(artifact: SignedArtifact, toBeSigned: Uint8Array, kid: Uint8Array) {
  const checked = verifyDocumentArtifact(artifact)
  ensure(
    equal(checked.toBeSigned, toBeSigned) && equal(checked.keyFingerprint, kid),
    'INTEGRITY_FAILED',
    '签名结果与批准的内容或密钥不符。'
  )
  return checked
}

export function prepareAttest(
  context: ExecutionContext,
  content: { origin: string; statement: DocumentStatement; devicePublicKey: Uint8Array },
  now = Date.now()
) {
  const { statement } = content
  ensure(statement.issuer === context.issuer, 'FORBIDDEN', '签署者必须与已认证账户一致。')
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
    return new PreparedAttestation(
      {
        kind: 'attest_app_action',
        request: {
          account_id: fixed(context.accountId, 12),
          issuer: context.issuer,
          action: actionToCandid(statement.content.action),
          signature: new Uint8Array(),
          approval: approval(context, now)
        }
      },
      context.homeUser,
      content.devicePublicKey
    )
  return new PreparedAttestation(
    {
      kind: 'attest',
      request: {
        account_id: fixed(context.accountId, 12),
        statement: toStatement(statement),
        origin: content.origin,
        signature: new Uint8Array(),
        approval: approval(context, now)
      }
    },
    context.homeUser,
    content.devicePublicKey
  )
}

/** A recovered device's vetKD derivation of the committed root generation. */
export class PreparedDerivation {
  readonly request: DeriveRootRequest
  private readonly message: Uint8Array
  private submission?: Promise<ExecutionResult>
  constructor(request: DeriveRootRequest, homeUser: Principal) {
    this.request = decodeControl('derive_root', encodeControl('derive_root', [request]))[0] as DeriveRootRequest
    this.message = deriveApprovalMessage(homeUser, this.request)
  }
  get requestId() {
    return hex(Uint8Array.from(this.request.approval.request_id))
  }
  get approvalMessage() {
    return Uint8Array.from(this.message)
  }
  approveAndExecute(
    user: Pick<UserService, 'derive_root'>,
    signer: DeviceSigner,
    onApproved: (request: DeriveRootRequest) => Promise<void> = async () => {}
  ): Promise<ExecutionResult> {
    this.submission ??= (async () => {
      const signed = decodeControl('derive_root', encodeControl('derive_root', [this.request]))[0] as DeriveRootRequest
      signed.approval.signature = fixed(await signer(this.approvalMessage), 64)
      await onApproved(decodeControl('derive_root', encodeControl('derive_root', [signed]))[0] as DeriveRootRequest)
      return submitDerivation(user, signed)
    })()
    return this.submission
  }
}
export async function submitDerivation(user: Pick<UserService, 'derive_root'>, signed: DeriveRootRequest) {
  let response: Awaited<ReturnType<UserService['derive_root']>>
  try {
    response = await user.derive_root(signed)
  } catch {
    throw new DmsgError(
      'EXECUTION_UNKNOWN',
      `执行结果尚未确认，请按请求 ${hex(Uint8Array.from(signed.approval.request_id))} 对账。`
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
    equal(Uint8Array.from(result.request_id), Uint8Array.from(signed.approval.request_id)),
    'INTEGRITY_FAILED',
    '响应与原请求不一致。'
  )
  return result
}
export function prepareRootDerivation(
  context: ExecutionContext,
  generation: bigint,
  transportPublicKey: Uint8Array,
  maxCycles: bigint,
  now = Date.now()
) {
  ensure(uint64(generation) > 0n, 'INVALID_INPUT')
  ensure(maxCycles > 0n && maxCycles <= 100000000000n, 'QUOTA_EXCEEDED')
  return new PreparedDerivation(
    {
      account_id: fixed(context.accountId, 12),
      generation,
      transport_public_key: fixed(transportPublicKey, 48),
      max_cycles: maxCycles,
      approval: approval(context, now)
    },
    context.homeUser
  )
}
/** Reads the stored result of an approved derivation, reconciling one still
 * in flight. Null means no execution is stored under this request ID. */
export async function recordedExecution(
  user: Pick<UserService, 'get_execution' | 'reconcile_execution'>,
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
/** The retained artifact of an attestation, or null when none is stored. */
export async function recordedAttestation(
  user: Pick<UserService, 'get_attestation'>,
  accountId: Uint8Array,
  requestId: Uint8Array
): Promise<SignedArtifact | null> {
  const response = await user.get_attestation(fixed(accountId, 12), fixed(requestId, 32))
  if ('Err' in response) {
    ensure('ResultExpired' in response.Err, 'EXECUTION_UNKNOWN')
    return null
  }
  return response.Ok
}
