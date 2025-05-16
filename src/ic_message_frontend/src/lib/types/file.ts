import { AesGcmKey } from '@ldclabs/cose-ts/aesgcm'

export class EncryptedFile extends File {
  constructor(
    fileBits: BlobPart[],
    fileName: string,
    options?: FilePropertyBag
  ) {
    super(fileBits, fileName, options)
  }

  static from(file: File) {
    return new EncryptedFile([file], file.name, {
      type: file.type,
      lastModified: file.lastModified
    })
  }

  static fromBlob(blob: Blob, name: string) {
    return new EncryptedFile([blob], name, {
      type: blob.type
    })
  }

  static fromRawBytes(data: Uint8Array, name: string, ctype: string) {
    return new EncryptedFile([data as BufferSource], name, {
      type: ctype
    })
  }

  static async fromEncrypt0Bytes(
    key: AesGcmKey,
    data: Uint8Array,
    aad: Uint8Array
  ) {}
}
