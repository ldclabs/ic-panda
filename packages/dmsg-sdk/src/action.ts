import { sha256 as rawHash } from "@noble/hashes/sha2.js";
import { concat, equalBytes } from "./encoding.ts";
/** Closed typed action validation. Successful validation is not product authorization. */
import type {
  AppAction,
  AppActionCommand,
  ActionFile,
  AppRegistration,
} from "./contracts.ts";
import { canonical, digest } from "./encoding.ts";
import {
  validateShape,
  validateIdentifier,
  validateOrigin,
  validateApp,
  requireValid,
} from "./validation.ts";

export const APP_ACTION_PROFILE = "application/vnd.dmsg.app-action+cose;v=1";
export const MAX_APP_ACTION_BYTES = 48 * 1024;
const text = (value: string, max: number) =>
  requireValid(
    /[^\p{White_Space}]/u.test(value) &&
      new TextEncoder().encode(value).length <= max &&
      !/[\p{Cc}]/u.test(value.replace(/[\n\t]/g, "")),
    "action text",
  );
const nonzero = (value: Uint8Array) =>
  requireValid(
    value.some((v) => v !== 0),
    "zero commitment",
  );
const media = (value: string) =>
  requireValid(
    value.length <= 128 &&
      /^[\x20-\x7e]+$/.test(value) &&
      /^[!#$%&'*+.^_`|~0-9a-z-]+\/[!#$%&'*+.^_`|~0-9a-z-]+(?:; *[!#$%&'*+.^_`|~0-9a-z-]+=(?:[!#$%&'*+.^_`|~0-9a-z-]+|"[^"\r\n]+"))*$/i.test(
        value,
      ),
    "action media type",
  );

export function validateActionFiles(files: ActionFile[]): void {
  validateShape("Vec<ActionFile>", files);
  let last: string | null = null;
  for (const file of files) {
    text(file.file_id, 128);
    requireValid(
      /^[A-Za-z0-9/_:.-]+$/.test(file.file_id) &&
        (last === null || last < file.file_id),
      "file order/duplicate",
    );
    last = file.file_id;
    requireValid(
      file.revision > 0n && file.byte_length <= 256n * 1024n * 1024n,
      "file revision/size",
    );
    nonzero(file.sha256);
    media(file.media_type);
    if (file.display_name !== null) text(file.display_name, 512);
  }
}

export function validateActionCommand(command: AppActionCommand): void {
  validateShape("AppActionCommand", command);
  const value = Object.values(command)[0]!;
  requireValid(value.project_id > 0n, "project id");
  if ("TokenListCertifyDisclosure" in command) {
    const c = command.TokenListCertifyDisclosure;
    requireValid(c.contract_id > 0n && c.revision > 0n, "disclosure version");
  } else if ("TokenListDecideReview" in command) {
    const c = command.TokenListDecideReview;
    requireValid(
      c.case_id > 0n && c.round > 0n && c.round <= 0xffffffffn,
      "review round",
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
    requireValid(c.transition_id > 0n, "transition id");
    text(c.rationale, 4096);
    nonzero(c.statement_hash);
    if ("analysis" in c && c.analysis !== null) {
      const a = c.analysis;
      const url = new URL(a.uri);
      requireValid(
        ["https:", "ipfs:"].includes(url.protocol) &&
          url.href === a.uri &&
          a.uri.length <= 4096 &&
          /^[\x21-\x7e]+$/.test(a.uri) &&
          !/%(?![0-9a-fA-F]{2})/.test(a.uri) &&
          !url.username &&
          !url.password,
        "artifact URI",
      );
      nonzero(a.sha256);
      media(a.content_type);
      requireValid(
        a.size > 0n && a.size <= 256n * 1024n * 1024n,
        "artifact size",
      );
    }
  }
}

export function validateAppAction(action: AppAction): void {
  validateShape("AppAction", action);
  requireValid(action.version === 1n, "unsupported action version");
  validateIdentifier(action.app_id);
  validateOrigin(action.origin, action.environment);
  requireValid(
    action.receiver.length > 0 &&
      !(action.receiver.length === 1 && action.receiver[0] === 4),
    "receiver",
  );
  nonzero(action.actor_id);
  nonzero(action.signing_account);
  requireValid(action.app_config_version > 0n, "app revision");
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
    nonzero(hash);
  requireValid(
    action.issued_at_ms < action.expires_at_ms &&
      action.expires_at_ms - action.issued_at_ms <= 300000n,
    "action window",
  );
  validateActionCommand(action.command);
  requireValid(
    equalBytes(action.input_hash, actionInputHash(action.command)),
    "action input commitment",
  );
  validateActionFiles(action.files);
  requireValid(canonical(action).length <= MAX_APP_ACTION_BYTES, "action size");
}

export function validateActionAdmission(
  action: AppAction,
  app: AppRegistration,
  nowMs: bigint,
): void {
  validateAppAction(action);
  validateApp(app);
  requireValid(
    !app.paused &&
      action.environment === app.environment &&
      action.app_id === app.app_id &&
      action.app_config_version === app.config_version &&
      app.origins.includes(action.origin) &&
      app.capabilities.includes("SignAction") &&
      app.profiles.includes("AppActionV1"),
    "action admission",
  );
  requireValid(
    action.issued_at_ms <= nowMs && nowMs < action.expires_at_ms,
    "action expired",
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
  const domain = new TextEncoder().encode("tokenlisting:signable-action:v1");
  return rawHash(
    concat(Uint8Array.of(domain.length), domain, canonical(input)),
  );
}
