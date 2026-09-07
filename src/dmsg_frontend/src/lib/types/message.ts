import { decode, encode } from 'cborg'

export type RawMessage = string | [string, MessageKind, Uint8Array]

export enum MessageKind {
  Default = '',
  File = 'File' // FilePayload
}

export interface FilePayload {
  canister: Uint8Array
  id: number
  name: string
  size: number
  type: string
}

export class MessageDetail<T> {
  message: string
  kind: MessageKind
  error: string = ''
  payload: T | null = null

  static from<T>(raw: Uint8Array, useMaps: boolean = false) {
    try {
      const rt: RawMessage = decode(raw, {
        useMaps,
        rejectDuplicateMapKeys: true
      })

      if (typeof rt === 'string') {
        return new MessageDetail<T>(rt)
      }

      if (!Array.isArray(rt) || rt.length != 3) {
        const msg = new MessageDetail<T>('')
        msg.error = 'Invalid message format'
        msg.payload = rt
        return msg
      }

      const msg = new MessageDetail<T>(rt[0], rt[1])
      try {
        msg.payload = decode(rt[2], {
          useMaps,
          rejectDuplicateMapKeys: true
        }) as T
      } catch (err) {
        msg.error = `Failed to decode payload: ${err}`
      }
      return msg
    } catch (err) {
      const msg = new MessageDetail<T>('')
      msg.error = `Failed to decode message: ${err}`
      return msg
    }
  }

  constructor(
    message: string,
    kind: MessageKind = MessageKind.Default,
    payload?: any
  ) {
    this.message = message
    this.kind = kind
    if (payload != null) {
      this.payload = payload
    }
  }

  asFile(): FilePayload | null {
    return this.kind === MessageKind.File ? (this.payload as FilePayload) : null
  }

  toBytes(): Uint8Array {
    if (this.kind == MessageKind.Default) {
      return encode(this.message, {})
    }
    return encode([this.message, this.kind, encode(this.payload, {})], {})
  }
}
