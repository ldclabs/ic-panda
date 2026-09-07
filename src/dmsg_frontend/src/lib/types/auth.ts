export interface AuthMessage<T> {
  kind: string
  error?: string
  result?: T
}
