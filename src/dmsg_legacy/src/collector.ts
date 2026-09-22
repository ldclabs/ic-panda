import { Principal } from '@icp-sdk/core/principal'
import {
  binary,
  digest,
  LegacyError,
  observation,
  pack,
  principal,
  readObservation,
  requireLegacy
} from './base'
import { type LegacyArchive, validateArchive } from './archive'
import {
  channelSettingKey,
  decodeLegacyMessage,
  legacyDecrypt,
  type CachedRoot,
  type LegacyRootSource,
  unlockLegacyChannel,
  unlockLegacyExchange,
  unlockLegacyRoot
} from './crypto'
import { LegacyLocalReader } from './local'
import { LegacyReader } from './reader'

/** Explicit user action. All output is sensitive and must immediately be sealed
 * to the confirmed extension recipient; never download this object as JSON. */
export async function collectArchive(
  reader: LegacyReader,
  input: {
    password?: string
    salt?: string
    keyId: Uint8Array
    contextVersion: 1 | 2
    keyName: string
  },
  signal?: AbortSignal
) {
  const inventory = await reader.collect(signal)
  const archive: LegacyArchive = {
    format: 'dmsg-legacy-archive/1',
    inventory,
    keys: [],
    checks: [],
    createdAt: Date.now()
  }
  const local = await LegacyLocalReader.open(reader.source.principal)
  try {
    if (local) {
      // Keep wrappers, history and in-progress uploads; omit cached password
      // derivations and bearer download tokens from content archives.
      for (const store of ['Keys', 'My', 'Channels', 'Messages'] as const) {
        for await (const entry of local.entries(store)) {
          requireLegacy(!signal?.aborted, 'cancelled', 'Archive paused')
          if (
            store === 'Keys' &&
            (typeof entry.key !== 'string' || !/^(MK$|EK$|KEK:)/.test(entry.key))
          )
            continue
          if (
            store === 'My' &&
            (typeof entry.key !== 'string' || !/^(User$|Profile$|KV$|File:)/.test(entry.key))
          )
            continue
          let value = entry.value
          if (entry.key === 'MK')
            value = (value as any[]).map(({ kind, keyId, encryptedSecret, version }) => ({
              kind,
              keyId,
              encryptedSecret,
              version: version || 1
            }))
          reader.add(
            `local/${store}/${digest(pack(observation(entry.key)))}`,
            'local',
            { key: entry.key, value },
            'local_cache'
          )
          if (store === 'Channels' || store === 'Messages') {
            const data = value as any
            const canister = data.canister
              ? principal(data.canister)
              : Principal.fromUint8Array(binary(data._canister)).toText()
            requireLegacy(
              reader.source.allowed.channel.includes(canister),
              'permission',
              'Cached channel is outside the source inventory'
            )
            if (store === 'Channels' && data.dek) {
              const key = `${canister}/channel/${data.id}`
              if (!inventory.objects.some((o) => o.key === key))
                reader.add(key, 'channel', data, 'local_cache')
            } else if (store === 'Messages') {
              const key = `${canister}/channel/${data.channel}/message/${data.id}`
              const message = {
                id: data.id,
                reply_to: data.reply_to,
                kind: data.kind,
                created_at: data.created_at,
                created_by: data.created_by,
                payload: binary(data.payload)
              }
              const checksum = digest(pack(observation(message))),
                remote = inventory.objects.find((o) => o.key === key)
              if (!remote || remote.digest !== checksum)
                reader.add(
                  remote ? `${key}/cache/${checksum}` : key,
                  'message',
                  message,
                  'local_cache'
                )
            }
          }
        }
      }
    } else if (reader.source.mode === 'Local')
      reader.gap(
        'local/browser',
        new LegacyError(
          'missing_key',
          'Original browser material unavailable; missing random KEKs cannot be regenerated'
        )
      )
    const userRecord = inventory.objects.find((o) => o.kind === 'identity')!
    const user = readObservation(userRecord.bytes)
    await reader.resources(signal)
    if (user.image) {
      try {
        const url = new URL(user.image),
          match = /^([a-z0-9-]+)\.(icp0\.io|ic0\.app)$/.exec(url.hostname),
          path = /^\/f\/([0-9]+)$/.exec(url.pathname)
        requireLegacy(
          url.protocol === 'https:' &&
            match &&
            path &&
            !url.username &&
            !url.password &&
            !url.port &&
            !url.searchParams.has('token'),
          'permission',
          'Avatar requires a reviewed public OSS source'
        )
        const bucket = principal(match[1]!),
          fileId = Number(path[1])
        const info = await reader.call('bucket', bucket, 'get_file_info', [fileId, []])
        requireLegacy(
          Number(info.size) <= 2 * 1024 * 1024,
          'limit',
          'Legacy avatar exceeds 2 MiB preview limit'
        )
        const file = await reader.file(bucket, fileId, null, signal)
        reader.add(`${bucket}/avatar/${fileId}`, 'avatar', {
          info: file.info,
          bytes: file.bytes,
          digest: file.digest,
          originalUrl: user.image
        })
      } catch (cause) {
        reader.gap('profile/avatar', cause)
      }
    }
    const cose = user.cose_canister?.[0] ? principal(user.cose_canister[0]) : null
    const myIV = binary(await reader.call('message', reader.source.message, 'my_iv'))
    const source: LegacyRootSource = {
      mode: reader.source.mode,
      principal: inventory.principal,
      keyId: input.keyId,
      myIV,
      canister: cose,
      namespace: inventory.principal.replaceAll('-', '_'),
      userOwned: true,
      settingVersion: 0,
      contextVersion: input.contextVersion,
      keyName: input.keyName
    }
    const cached = ((await local?.get('Keys', 'MK')) as CachedRoot[] | undefined)
      ?.filter((k) => k.kind === source.mode)
      .at(-1)
    reader.add(`${source.principal}/root-descriptor`, 'setting', {
      source,
      cached: cached
        ? {
            kind: cached.kind,
            keyId: cached.keyId,
            encryptedSecret: cached.encryptedSecret,
            version: cached.version ?? 1
          }
        : null
    })
    let root: Awaited<ReturnType<typeof unlockLegacyRoot>> | null = null
    try {
      root = await unlockLegacyRoot(reader, source, {
        password: input.password,
        salt: input.salt,
        cached
      })
    } catch (cause) {
      reader.gap(`${source.principal}/master`, cause)
    }
    if (root)
      archive.keys.push({
        source: `${source.principal}/master`,
        principal: source.principal,
        purpose: 'master',
        coseKey: root.key.toBytes(),
        parameters: pack(
          observation({
            ...source,
            path: root.path,
            publicKeyDigest: root.publicKeyDigest,
            originalWrapper: root.originalWrapper
          })
        )
      })
    // Static ECDH is historical decryption material, never an II/authentication key.
    try {
      requireLegacy(root, 'missing_key', 'Historical master key unavailable')
      let wrapped = (await local?.get('Keys', 'EK')) as Uint8Array | undefined
      if (!wrapped && cose) {
        const setting = await reader.setting(
          { ...root.path, key: new TextEncoder().encode('StaticECDH') },
          cose
        )
        wrapped = setting.value.dek[0]
      }
      if (wrapped)
        archive.keys.push({
          source: `${source.principal}/StaticECDH`,
          principal: source.principal,
          purpose: 'static_ecdh',
          coseKey: await legacyDecrypt(
            root.key,
            binary(wrapped),
            Principal.fromText(source.principal).toUint8Array()
          ),
          parameters: pack(observation(source))
        })
    } catch (cause) {
      reader.gap(`${source.principal}/StaticECDH`, cause)
    }
    for (const record of inventory.objects.filter((o) => o.kind === 'channel')) {
      requireLegacy(!signal?.aborted, 'cancelled', 'Archive paused')
      const channel = readObservation(record.bytes),
        canister = principal(channel.canister)
      let keys: Awaited<ReturnType<typeof unlockLegacyChannel>> | null = null
      try {
        try {
          requireLegacy(root, 'missing_key', 'Historical master key unavailable')
          let wrapped = (await local?.get('Keys', `KEK:${canister}:${channel.id}`)) as
            Uint8Array | undefined
          let path = { ...root.path, key: channelSettingKey(canister, channel.id) }
          if (!wrapped && cose) {
            try {
              const setting = await reader.setting(path, cose)
              path = setting.path as typeof path
              wrapped = setting.value.dek[0]
            } catch (cause) {
              if (!(cause instanceof LegacyError) || cause.code !== 'missing') throw cause
            }
          }
          const staticKey = archive.keys.find((k) => k.purpose === 'static_ecdh')
          const pending = channel.my_setting?.ecdh_remote?.[0],
            recipient = channel.my_setting?.ecdh_pub?.[0]
          keys =
            !wrapped && staticKey && pending && recipient
              ? await unlockLegacyExchange(
                  staticKey.coseKey,
                  binary(recipient),
                  [binary(pending[0]), binary(pending[1])],
                  binary(channel.dek)
                )
              : await unlockLegacyChannel(
                  root.key,
                  wrapped ? binary(wrapped) : null,
                  binary(channel.dek)
                )
          for (const [purpose, key] of [
            ['channel_kek', keys.kek],
            ['channel_dek', keys.dek]
          ] as const)
            archive.keys.push({
              source: record.key,
              principal: source.principal,
              purpose,
              coseKey: key.toBytes(),
              parameters: pack(
                observation({
                  canister,
                  id: channel.id,
                  path,
                  wrapped: wrapped ?? null,
                  exchange: !wrapped ? channel.my_setting : null,
                  dek: channel.dek
                })
              )
            })
        } catch (cause) {
          reader.gap(record.key, cause)
        }
        const attachments = new Map<
          string,
          { canister: string; id: number; size: number | null }
        >()
        for (const object of inventory.objects.filter(
          (o) => o.kind === 'message' && o.key.startsWith(`${record.key}/message/`)
        )) {
          try {
            const message = readObservation(object.bytes)
            const decoded = await decodeLegacyMessage(keys?.dek ?? null, {
              kind: message.kind,
              payload: binary(message.payload)
            })
            archive.checks.push({
              source: object.key,
              digest: object.digest,
              result: decoded.deleted
                ? 'deleted'
                : message.kind === 1
                  ? 'source_plaintext'
                  : 'aead_verified',
              plaintextDigest: decoded.deleted ? null : digest(pack(decoded))
            })
            if (decoded.file) {
              const bucket = Principal.fromUint8Array(decoded.file.canister).toText()
              attachments.set(`${bucket}/file/${decoded.file.id}`, {
                canister: bucket,
                id: decoded.file.id,
                size: decoded.file.size
              })
            }
          } catch (cause) {
            reader.gap(object.key, cause)
          }
        }
        if (attachments.size || channel.files_state?.length) {
          const token = await reader.call('channel', canister, 'download_files_token', [
            channel.id
          ])
          const bucket = principal(token.storage[0]),
            folder = token.storage[1]
          if (channel.files_state?.length)
            requireLegacy(
              principal(channel.files_state[0].file_storage[0]) === bucket &&
                channel.files_state[0].file_storage[1] === folder,
              'permission',
              'OSS source folder changed'
            )
          const access = binary(token.access_token)
          const listing = await reader.call('bucket', bucket, 'get_folder_info', [
            folder,
            [access]
          ])
          requireLegacy(
            listing.id === folder && listing.files.length <= 10000,
            'corrupt',
            'Invalid OSS folder listing'
          )
          for (const id of listing.files) {
            const key = `${bucket}/file/${id}`
            if (!attachments.has(key))
              attachments.set(key, { canister: bucket, id, size: null })
          }
          for (const [key, file] of attachments) {
            try {
              requireLegacy(
                file.canister === bucket,
                'permission',
                'Attachment bucket differs from channel storage'
              )
              const copied = await reader.file(file.canister, file.id, access, signal, true)
              requireLegacy(
                copied.info.parent === folder,
                'permission',
                'Attachment is outside channel folder'
              )
              reader.add(key, 'file', {
                info: copied.info,
                ciphertext: copied.bytes,
                chunks: copied.chunks,
                complete: copied.complete,
                present: copied.present,
                missing: copied.missing,
                channel: record.key
              })
              requireLegacy(
                copied.complete,
                'missing',
                'Incomplete upload: available ciphertext chunks retained'
              )
              requireLegacy(
                keys,
                'missing_key',
                'Ciphertext retained; historical channel DEK missing'
              )
              const plain = await legacyDecrypt(keys.dek, copied.bytes)
              if (file.size !== null)
                requireLegacy(
                  plain.length === file.size,
                  'corrupt',
                  'Attachment plaintext size mismatch'
                )
              const object = inventory.objects.find((o) => o.key === key)!
              if (!archive.checks.some((c) => c.source === key))
                archive.checks.push({
                  source: key,
                  digest: object.digest,
                  result: 'aead_verified',
                  plaintextDigest: digest(plain)
                })
              plain.fill(0)
            } catch (cause) {
              reader.gap(key, cause)
            }
          }
          const after = await reader.call('bucket', bucket, 'get_folder_info', [
            folder,
            [access]
          ])
          if (digest(pack(observation(after))) !== digest(pack(observation(listing))))
            reader.gap(
              `${bucket}/folder/${folder}`,
              new LegacyError('version', 'Folder changed during pre-migration snapshot')
            )
        }
      } catch (cause) {
        reader.gap(record.key, cause)
      }
    }
    return validateArchive(archive)
  } finally {
    local?.close()
  }
}
