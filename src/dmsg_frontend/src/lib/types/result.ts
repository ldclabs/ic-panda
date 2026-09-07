export interface Ok<T> {
  Ok: T
}

export interface Err<T> {
  Err: T
}

export type Result<T, E> = Ok<T> | Err<E>

export class ErrData<T> extends Error {
  data?: T
  static from(msg: string, data?: any) {
    const err = new ErrData(msg)
    if (data) {
      err.data = data
    }
    return err
  }
}

export function unwrapResult<T, E>(
  res: Result<T, E>,
  msg: string = 'error result'
): T {
  if ('Err' in res) {
    throw ErrData.from(msg, res.Err)
  }

  return res.Ok
}

export function unwrapNotFound<T, E>(
  res: Result<T, E>,
  msg: string = 'error result'
): T | null {
  if ('Err' in res) {
    const err = String(res.Err)
    if (err.includes('NotFound') || err.includes('not found')) {
      return null
    }

    throw ErrData.from(msg, res.Err)
  }

  return res.Ok
}

export function unwrapOption<T>(res: [] | [T]): T | null {
  if (Array.isArray(res) && res.length == 1) {
    return res[0]
  }

  return null
}

export function unwrapOptionResult<T, E>(
  res: Result<[] | [T], E>,
  msg: string = 'error result'
): T | null {
  if ('Err' in res) {
    throw ErrData.from(msg, res.Err)
  }

  return unwrapOption(res.Ok)
}

export function errMessage(err: any): string {
  if (err?.data) {
    return JSON.stringify(err.data, (key, value) =>
      typeof value === 'bigint' ? value.toString() : value
    )
  }
  if (err?.message) {
    return err.message
  }
  return String(err)
}
