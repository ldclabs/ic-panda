import { DmsgError } from '@dmsg/sdk/errors'

// SDK codec/statement failures and application checks share one coded error type.
export { DmsgError }
export function ensure(condition: unknown, code: string, message?: string): asserts condition {
  if (!condition) throw new DmsgError(code, message)
}
export const errorText = (error: unknown) =>
  error instanceof Error ? error.message : '操作未完成，请重试。'
