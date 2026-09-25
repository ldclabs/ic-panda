/** Closed typed action validation. Successful validation is not product authorization. */
import type {
  AppAction,
  AppActionCommand,
  ActionFile,
  AppRegistration,
} from "./contracts.ts";
import { ensure } from "./cose-errors.ts";
import {
  canonical,
  concat,
  digest,
  equalBytes,
  sha256,
  utf8,
} from "./encoding.ts";
import {
  AUTH_TTL_MS,
  validateShape,
  validateIdentifier,
  validateOrigin,
  validateApp,
  validatePrincipal,
  validateNonzero,
} from "./validation.ts";

export const APP_ACTION_PROFILE = "application/vnd.dmsg.app-action+cose;v=1";
export const MAX_APP_ACTION_BYTES = 48 * 1024;
/** RFC 9110 media type with optional parameters. */
export const MEDIA_TYPE_PATTERN =
  /^[!#$%&'*+.^_`|~0-9a-z-]+\/[!#$%&'*+.^_`|~0-9a-z-]+(?:; *[!#$%&'*+.^_`|~0-9a-z-]+=(?:[!#$%&'*+.^_`|~0-9a-z-]+|"[^"\r\n]+"))*$/i;
const ACTION_DOMAIN = utf8("tokenlisting:signable-action:v1");
const text = (value: string, max: number) =>
  ensure(
    /[^\p{White_Space}]/u.test(value) &&
      utf8(value).length <= max &&
      !/[\p{Cc}]/u.test(value.replace(/[\n\t]/g, "")),
    "INVALID_INPUT",
  );
const media = (value: string) =>
  ensure(
    value.length <= 128 &&
      /^[\x20-\x7e]+$/.test(value) &&
      MEDIA_TYPE_PATTERN.test(value),
    "INVALID_INPUT",
  );

export function validateActionFiles(files: ActionFile[]): void {
  validateShape("Vec<ActionFile>", files);
  let last: string | null = null;
  for (const file of files) {
    text(file.file_id, 128);
    ensure(
      /^[A-Za-z0-9/_:.-]+$/.test(file.file_id) &&
        (last === null || last < file.file_id),
      "INVALID_INPUT",
    );
    last = file.file_id;
    ensure(
      file.revision > 0n && file.byte_length <= 256n * 1024n * 1024n,
      "INVALID_INPUT",
    );
    validateNonzero(file.sha256);
    media(file.media_type);
    if (file.display_name !== null) text(file.display_name, 512);
  }
}

export function validateActionCommand(command: AppActionCommand): void {
  validateShape("AppActionCommand", command);
  const value = Object.values(command)[0]!;
  ensure(value.project_id > 0n, "INVALID_INPUT");
  if ("TokenListCertifyDisclosure" in command) {
    const c = command.TokenListCertifyDisclosure;
    ensure(c.contract_id > 0n && c.revision > 0n, "INVALID_INPUT");
  } else if ("TokenListDecideReview" in command) {
    const c = command.TokenListDecideReview;
    ensure(
      c.case_id > 0n && c.round > 0n && c.round <= 0xffffffffn,
      "INVALID_INPUT",
    );
    text(c.rationale, 4096);
    for (const change of c.changes) {
      text(change.locator, 128);
      text(change.detail, 4096);
    }
  } else {
    const c =
      "TokenListCertifyTransition" in command
        ? command.TokenListCertifyTransition
        : command.TokenListApproveTransition;
    ensure(c.transition_id > 0n, "INVALID_INPUT");
    text(c.rationale, 4096);
    validateNonzero(c.statement_hash);
    if ("analysis" in c && c.analysis !== null) {
      const a = c.analysis;
      ensure(URL.canParse(a.uri), "INVALID_INPUT");
      const url = new URL(a.uri);
      ensure(
        ["https:", "ipfs:"].includes(url.protocol) &&
          url.href === a.uri &&
          a.uri.length <= 4096 &&
          /^[\x21-\x7e]+$/.test(a.uri) &&
          !/%(?![0-9a-fA-F]{2})/.test(a.uri) &&
          !url.username &&
          !url.password,
        "INVALID_INPUT",
      );
      validateNonzero(a.sha256);
      media(a.content_type);
      ensure(a.size > 0n && a.size <= 256n * 1024n * 1024n, "INVALID_INPUT");
    }
  }
}

export function validateAppAction(action: AppAction): void {
  validateShape("AppAction", action);
  ensure(action.version === 1n, "UNSUPPORTED_PROTOCOL");
  validateIdentifier(action.app_id);
  validateOrigin(action.origin, action.environment);
  validatePrincipal(action.receiver);
  validateNonzero(action.actor_id);
  validateNonzero(action.signing_account);
  ensure(action.app_config_version > 0n, "INVALID_INPUT");
  for (const hash of [
    action.operation_id,
    action.intent_hash,
    action.input_hash,
    action.subject_hash,
    action.precondition_hash,
    action.role_snapshot_hash,
    action.signing_policy_hash,
    action.rule_set_hash,
  ])
    validateNonzero(hash);
  ensure(
    action.issued_at_ms < action.expires_at_ms &&
      action.expires_at_ms - action.issued_at_ms <= AUTH_TTL_MS,
    "INVALID_INPUT",
  );
  validateActionCommand(action.command);
  ensure(
    equalBytes(action.input_hash, actionInputHash(action.command)),
    "INTEGRITY_FAILED",
  );
  validateActionFiles(action.files);
  ensure(
    canonical(action).length <= MAX_APP_ACTION_BYTES,
    "QUOTA_EXCEEDED",
  );
}

export function validateActionAdmission(
  action: AppAction,
  app: AppRegistration,
  nowMs: bigint,
): void {
  validateAppAction(action);
  validateApp(app);
  ensure(!app.paused, "LOCKED");
  ensure(
    action.environment === app.environment &&
      action.app_id === app.app_id &&
      action.app_config_version === app.config_version &&
      app.origins.includes(action.origin) &&
      app.capabilities.includes("SignAction") &&
      app.profiles.includes("AppActionV1"),
    "FORBIDDEN",
  );
  ensure(
    action.issued_at_ms <= nowMs && nowMs < action.expires_at_ms,
    "EXPIRED",
  );
}

/** Complete deterministic commitment; authenticate its source independently. */
export const appActionDigest = (action: AppAction) =>
  digest("dmsg/app-action/v1", action);

/** Reconstruct the complete product command, independent of its display and receiver. */
export function actionInputHash(command: AppActionCommand): Uint8Array {
  let input: unknown;
  if ("TokenListCertifyDisclosure" in command) {
    const c = command.TokenListCertifyDisclosure;
    input = {
      CertifyDisclosure: {
        contract: c.contract_id,
        expected_revision: c.revision,
      },
    };
  } else if ("TokenListDecideReview" in command) {
    const c = command.TokenListDecideReview;
    input = {
      DecideReviewCase: {
        case: c.case_id,
        outcome: c.outcome,
        changes: c.changes,
        rationale: c.rationale,
      },
    };
  } else if ("TokenListCertifyTransition" in command) {
    const c = command.TokenListCertifyTransition;
    input = {
      CertifyTransition: {
        transition: c.transition_id,
        statement_hash: c.statement_hash,
        rationale: c.rationale,
        analysis: c.analysis,
      },
    };
  } else {
    const c = command.TokenListApproveTransition;
    input = {
      ApproveTransition: {
        transition: c.transition_id,
        approve: c.approve,
        statement_hash: c.statement_hash,
        rationale: c.rationale,
      },
    };
  }
  return sha256(
    concat(
      Uint8Array.of(ACTION_DOMAIN.length),
      ACTION_DOMAIN,
      canonical(input),
    ),
  );
}
