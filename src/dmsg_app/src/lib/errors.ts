export class DmsgError extends Error {
  constructor(
    public readonly code: string,
    message = code
  ) {
    super(message)
    this.name = 'DmsgError'
  }
}
export function ensure(condition: unknown, code: string, message?: string): asserts condition {
  if (!condition) throw new DmsgError(code, message)
}
export const errorText = (error: unknown) =>
  error instanceof Error ? error.message : '操作未完成，请重试。'
