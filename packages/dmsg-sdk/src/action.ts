/** Application-action validation against a registered schema. Successful validation is not product authorization. */
import type {
  AppAction,
  ActionArg,
  ActionArtifact,
  ActionFile,
  ActionSchema,
  ActionValue,
  AppRegistration,
  ChoiceOption,
  FieldSchema,
  FieldType,
} from "./contracts.ts";
import { ensure } from "./cose-errors.ts";
import { canonical, digest, equalBytes } from "./encoding.ts";
import {
  AUTH_TTL_MS,
  MAX_ACTION_DEPTH,
  actionName,
  actionText,
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
const media = (value: string) =>
  ensure(
    value.length <= 128 &&
      /^[\x20-\x7e]+$/.test(value) &&
      MEDIA_TYPE_PATTERN.test(value),
    "INVALID_INPUT",
  );

export function validateActionArtifact(a: ActionArtifact): void {
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

export function validateActionFiles(files: ActionFile[]): void {
  validateShape("Vec<ActionFile>", files);
  let last: string | null = null;
  for (const file of files) {
    actionText(file.file_id, 128, false);
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
    if (file.display_name !== null) actionText(file.display_name, 512, true);
  }
}

/** Bounds nesting and counts before the recursive shape check runs. */
function bounded(args: unknown, level: number): void {
  ensure(Array.isArray(args) && args.length <= 32, "INVALID_INPUT");
  for (const arg of args) {
    ensure(arg && typeof arg === "object", "INVALID_INPUT");
    boundedValue((arg as { value?: unknown }).value, level);
  }
}

function boundedValue(value: unknown, level: number): void {
  ensure(level <= MAX_ACTION_DEPTH, "INVALID_INPUT");
  if (!value || typeof value !== "object") return;
  if (Object.hasOwn(value, "List")) {
    const items = (value as { List: unknown }).List;
    ensure(Array.isArray(items) && items.length <= 64, "INVALID_INPUT");
    for (const item of items) boundedValue(item, level + 1);
  } else if (Object.hasOwn(value, "Record"))
    bounded((value as { Record: unknown }).Record, level + 1);
}

function value(v: ActionValue): void {
  if (v === "Null") return;
  if ("Text" in v) actionText(v.Text, 4096, true);
  else if ("Choice" in v) actionName(v.Choice);
  else if ("List" in v) v.List.forEach(value);
  else if ("Record" in v) values(v.Record);
}

function values(args: ActionArg[]): void {
  for (const arg of args) {
    actionName(arg.name);
    value(arg.value);
  }
}

/** Schema-independent content checks; historical signatures remain verifiable. */
export function validateAppAction(action: AppAction): void {
  ensure(action && typeof action === "object", "INVALID_INPUT");
  bounded((action.command as { args?: unknown } | undefined)?.args, 1);
  validateShape("AppAction", action);
  ensure(action.version === 1n, "UNSUPPORTED_PROTOCOL");
  validateIdentifier(action.app_id);
  validateOrigin(action.origin, action.environment);
  validatePrincipal(action.receiver);
  ensure(action.actor.length >= 1 && action.actor.length <= 64, "INVALID_INPUT");
  validateNonzero(action.actor);
  validateNonzero(action.signing_account);
  ensure(action.app_config_version > 0n, "INVALID_INPUT");
  for (const hash of [
    action.operation_id,
    action.intent_hash,
    action.schema_hash,
  ])
    validateNonzero(hash);
  ensure(
    action.issued_at_ms < action.expires_at_ms &&
      action.expires_at_ms - action.issued_at_ms <= AUTH_TTL_MS,
    "INVALID_INPUT",
  );
  actionName(action.command.name);
  values(action.command.args);
  validateActionFiles(action.files);
  ensure(
    canonical(action).length <= MAX_APP_ACTION_BYTES,
    "QUOTA_EXCEEDED",
  );
}

/** Digest naming one exact schema; every action signs the digest of its schema. */
export const actionSchemaHash = (schema: ActionSchema) =>
  digest("dmsg/action-schema/v1", schema);

/** The schema entry describing the action's command, after checking the signed digest. */
export function actionCommandSchema(action: AppAction, schema: ActionSchema) {
  ensure(
    equalBytes(action.schema_hash, actionSchemaHash(schema)),
    "INTEGRITY_FAILED",
  );
  const command = schema.commands.find((c) => c.name === action.command.name);
  ensure(command, "UNSUPPORTED_PROTOCOL");
  return command;
}

function conformFields(fields: FieldSchema[], args: ActionArg[]): void {
  ensure(fields.length === args.length, "INVALID_INPUT");
  fields.forEach((field, i) => {
    ensure(field.name === args[i]!.name, "INVALID_INPUT");
    conform(field.ty, args[i]!.value);
  });
}

function conform(ty: FieldType, v: ActionValue): void {
  if (typeof ty === "object" && "Optional" in ty) {
    if (v !== "Null") conform(ty.Optional.item, v);
    return;
  }
  ensure(v !== "Null", "INVALID_INPUT");
  // Field types and values share variant names.
  const kind = typeof ty === "string" ? ty : Object.keys(ty)[0]!;
  ensure(Object.hasOwn(v, kind), "INVALID_INPUT");
  const x = (v as Record<string, any>)[kind];
  const t = typeof ty === "string" ? null : (ty as Record<string, any>)[kind];
  if (kind === "Nat") ensure(t.min <= x && x <= t.max, "INVALID_INPUT");
  else if (kind === "Text") actionText(x, Number(t.max_bytes), t.multiline);
  else if (kind === "Hash") validateNonzero(x);
  else if (kind === "Principal") validatePrincipal(x);
  else if (kind === "Choice")
    ensure(
      t.options.some((o: ChoiceOption) => o.value === x),
      "INVALID_INPUT",
    );
  else if (kind === "Artifact") validateActionArtifact(x);
  else if (kind === "List") {
    ensure(BigInt(x.length) <= t.max_items, "INVALID_INPUT");
    for (const item of x) conform(t.item, item);
  } else if (kind === "Record") conformFields(t.fields, x);
}

/** Check the command against the exact schema it signs; `schema` must be an accepted registration. */
export function validateActionCommand(
  action: AppAction,
  schema: ActionSchema,
): void {
  conformFields(actionCommandSchema(action, schema).fields, action.command.args);
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
    app.action_schema !== null &&
      action.environment === app.environment &&
      action.app_id === app.app_id &&
      action.app_config_version === app.config_version &&
      app.origins.includes(action.origin) &&
      app.capabilities.includes("SignAction") &&
      app.profiles.includes("AppActionV1"),
    "FORBIDDEN",
  );
  validateActionCommand(action, app.action_schema!);
  ensure(
    action.issued_at_ms <= nowMs && nowMs < action.expires_at_ms,
    "EXPIRED",
  );
}

/** Complete deterministic commitment; authenticate its source independently. */
export const appActionDigest = (action: AppAction) =>
  digest("dmsg/app-action/v1", action);
