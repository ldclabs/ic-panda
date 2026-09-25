/** Stable protocol error. `code` is machine-readable; callers may add a display message. */
export class DmsgError extends Error {
  readonly code: string;
  constructor(code: string, message = code) {
    super(message);
    this.code = code;
    this.name = "DmsgError";
  }
}

/** SDK checks report the Rust protocol's language-neutral codes, without any UI dependency. */
export function ensure(value: unknown, code: string): asserts value {
  if (!value) throw new DmsgError(code);
}
