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
  msg: string = 'error result',
  opt = false
): T {
  if ('Err' in res) {
    throw ErrData.from(msg, res.Err)
  }

  if (opt && Array.isArray(res.Ok)) {
    return res.Ok[0] || null
  }

  return res.Ok
}

export function errMessage(err: any): string {
  console.error(err)
  if (err?.data) {
    return JSON.stringify(err.data)
  }
  if (err?.message) {
    return err.message
  }
  return String(err)
}
