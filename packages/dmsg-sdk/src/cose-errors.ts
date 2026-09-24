/** Stable protocol error. `code` is machine-readable; callers may add a display message. */
export class DmsgError extends Error {
  readonly code: string;
  constructor(code: string, message = code) {
    super(message);
    this.code = code;
    this.name = "DmsgError";
  }
}

/** SDK checks report language-neutral codes, without any UI, account store or wallet dependency. */
export function ensure(
  value: unknown,
  code: string,
  _detail?: string,
): asserts value {
  if (!value) throw new DmsgError(code);
}
