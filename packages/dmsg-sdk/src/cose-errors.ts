/** Stable protocol errors without any UI, account store or wallet dependency. */
export function ensure(
  value: unknown,
  code: string,
  _detail?: string,
): asserts value {
  if (!value) throw new Error(code);
}
