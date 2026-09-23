import { Principal } from '@icp-sdk/core/principal'
import { InboxClient } from '../src/lib/services/inbox'
import { idlFactory as paymentIdl } from '../src/lib/canisters/generated/payment/index.js'
import type { _SERVICE as PaymentService } from '../src/lib/canisters/generated/payment'
import { CommerceClient } from '../src/lib/services/commerce'
import { WalletClient } from '../src/lib/services/wallet'
import { idlFactory as commerceIdl } from '../src/lib/canisters/generated/commerce/index.js'
import { idlFactory as membershipIdl } from '../src/lib/canisters/generated/membership/index.js'
import type { _SERVICE as CommerceService } from '../src/lib/canisters/generated/commerce'
import type { _SERVICE as MembershipService } from '../src/lib/canisters/generated/membership'
import { IDL } from '@icp-sdk/core/candid'
import {
  pack as legacyPack,
  observation,
  digest as legacyDigest,
  encodeArchive,
  sealTransfer,
  sharedSourceKey,
  type LegacyArchive
} from '@dmsg/legacy'
import { SharedMigrationClient, encodeLegacyProof } from '../src/lib/services/shared-migration'
import { collectSnapshot } from '@dmsg/legacy'
import { ChannelClient } from '../src/lib/services/channel'
import { Actor, HttpAgent } from '@icp-sdk/core/agent'
import { Ed25519KeyIdentity } from '@icp-sdk/core/identity'
import { idlFactory } from '../src/lib/canisters/generated/user/index.js'
import type { _SERVICE } from '../src/lib/canisters/generated/user'
import { CryptoClient } from '../src/lib/crypto/client'
import { AccountClient } from '../src/lib/services/account'
import { AccountRootClient } from '../src/lib/services/account-root'
import { CloudClient } from '../src/lib/services/relay'
import { HandleClient } from '../src/lib/services/handle'
import { idlFactory as handleIdl } from '../src/lib/canisters/generated/handle/index.js'
import type { _SERVICE as HandleService } from '../src/lib/canisters/generated/handle'
import { SigningClient } from '../src/lib/services/signing'
import { idlFactory as coseIdl } from '../src/lib/canisters/generated/cose/index.js'
import type { _SERVICE as CoseService } from '../src/lib/canisters/generated/cose'
import {
  parseRequest,
  type PendingRequest,
  type SignatureRequest
} from '../src/lib/protocol/requests'
import { hpkeSeal } from '../src/lib/crypto/primitives'
import { canonical, id } from '../src/lib/protocol/codec'
import { ContentClient } from '../src/lib/services/content'
import { flushCipherDispatches } from '../src/lib/services/background'
import { decodeControlResult } from '../src/lib/protocol/account'
import { unb64, hex, hash, unhex } from '../src/lib/protocol/codec'
import { xidBytes } from '../src/lib/protocol/identity'
import { currentWorkspace, registry, WorkspaceDB } from '../src/lib/db'

type ProbeInput = {
  gateway: string
  user: string
  cose: string
  handle: string
  relay: string
  commerce: string
  membership: string
  ledger: string
  ledger_usdt: string
  payment: string
  identitySeed?: number
  legacy?: {
    gateway: string
    channel: string
    identity: string
    channel_id: number
    root_key_hex: string
    kek_hex: string
    dek_hex: string
  }
}
let authSeed = 42
let oldEvidence: {
  info: Awaited<ReturnType<_SERVICE['get_account']>>
  batch: Awaited<ReturnType<_SERVICE['security_snapshot_batch']>>
  bundle: Awaited<ReturnType<_SERVICE['get_device_bundle']>>
} | null = null
let saved: {
  input: ProbeInput
  crypto: CryptoClient
  password: string
  account: string
  code: string
  backup: string
} | null = null
async function reconnect() {
  if (!saved) throw new Error('Probe not initialized')
  const meta = await saved.crypto.call('unlock', saved.password)
  const identity = Ed25519KeyIdentity.generate(new Uint8Array(32).fill(authSeed))
  const agent = await HttpAgent.create({
    host: saved.input.gateway,
    identity,
    shouldFetchRootKey: false,
    verifyQuerySignatures: true
  })
  await agent.fetchRootKey()
  const actor = Actor.createActor<_SERVICE>(idlFactory, {
    agent,
    canisterId: saved.input.user
  })
  return new AccountClient(
    actor,
    agent,
    identity.getPrincipal(),
    saved.crypto,
    meta,
    saved.input.user
  )
}
;(window as any).accountFollowup = async (action: string, payload: any) => {
  if (action === 'fixture')
    return {
      input: saved!.input,
      account: saved!.account,
      code: saved!.code,
      backup: saved!.backup
    }
  if (action === 'restore') {
    authSeed = payload.input.identitySeed ?? 42
    const crypto = new CryptoClient(),
      password = 'independent-device-test-password'
    const restored = await crypto.call('restore', {
      file: new File([payload.backup], 'account.dmsg'),
      code: payload.code,
      password
    })
    saved = {
      input: payload.input,
      crypto,
      password,
      account: payload.account,
      code: payload.code,
      backup: payload.backup
    }
    const view = await crypto.call('view')
    await crypto.lock()
    return {
      device: restored.meta.deviceId,
      authorized: restored.meta.registered,
      entries: view.entries.length,
      generation: restored.meta.rootGeneration
    }
  }
  if (action === 'channel' && payload.action === 'offline') {
    await saved!.crypto.call('unlock', saved!.password)
    try {
      return {
        channels: await saved!.crypto.call('channelList'),
        messages: await saved!.crypto.call('channelMessages', payload.id),
        files: (await saved!.crypto.call('view')).entries
          .filter((e) => e.item.file)
          .map((e) => e.item.title)
      }
    } finally {
      await saved!.crypto.lock()
    }
  }
  if (action === 'request-recovery') authSeed = 43
  const client = await reconnect(),
    data = saved!
  const timer = setInterval(() => void data.crypto.call('tick').catch(() => {}), 5000)
  try {
    if (action === 'inbox') {
      const payment = Actor.createActor<PaymentService>(paymentIdl, {
        agent: client.agent,
        canisterId: data.input.payment
      })
      const inbox = new InboxClient(
        client,
        new CloudClient({ origin: data.input.relay, environment: 'local' }),
        payment,
        data.input.payment,
        data.account
      )
      if (payload.action === 'capability') {
        const state = await client.refresh(data.account)
        await client.mutate(data.account, {
          SetDeviceCapabilities: {
            device_id: unhex(client.meta.deviceId),
            capabilities: [...state.device!.input.capabilities, { PaymentOffer: null }]
          }
        })
        return true
      }
      if (payload.action === 'configure') {
        const post = inbox.cloud.post.bind(inbox.cloud)
        let lost = false
        inbox.cloud.post = async (...args) => {
          const result = await post(...args)
          if (!lost && args[0].endsWith('/policy')) {
            lost = true
            throw new Error('Policy response lost')
          }
          return result
        }
        await inbox.configure('paid', client.caller.toText(), '1000000').catch((error) => {
          if (error.message !== 'Policy response lost') throw error
        })
        const result = await inbox.configure('paid', client.caller.toText(), '1000000')
        if (!lost || result.version !== 1) throw new Error('Policy retry changed its version')
        return result
      }
      if (payload.action === 'resume') {
        const pending = await inbox.outgoing()
        if (pending.length !== 1) throw new Error('Missing durable outgoing contact')
        return await inbox.resume(pending[0].id)
      }
      if (payload.action === 'contact')
        return await inbox.contact(
          payload.recipient,
          'Private paid contact',
          client.caller.toText()
        )
      if (payload.action === 'pay-admit') {
        const post = inbox.cloud.post.bind(inbox.cloud)
        let lost = false
        inbox.cloud.post = async (...args) => {
          const result = await post(...args)
          if (!lost && args[0].endsWith('/admit')) {
            lost = true
            throw new Error('Admission response lost')
          }
          return result
        }
        const escrow = await inbox.openEscrow(payload.order)
        const funded = await inbox.fund(
          escrow,
          new WalletClient(client.agent, client.caller, data.crypto)
        )
        await inbox.admit(payload.order, funded).catch((error) => {
          if (error.message !== 'Admission response lost') throw error
        })
        const result = await inbox.admit(payload.order, funded)
        if (!lost) throw new Error('Admission fault was not exercised')
        return { state: result.order.state, decision: Object.keys(result.escrow.decision)[0] }
      }
      if (payload.action === 'list')
        return (await inbox.list()).map((value) => ({
          text: value.text,
          state: value.state,
          sender: value.sender
        }))
    }
    if (action === 'commerce') {
      const approved = await client.refresh(data.account)
      if (!approved.device!.input.capabilities.some((c) => 'FormalApprove' in c))
        await client.mutate(data.account, {
          SetDeviceCapabilities: {
            device_id: unhex(client.meta.deviceId),
            capabilities: [...approved.device!.input.capabilities, { FormalApprove: null }]
          }
        })
      const walletIdentity = Ed25519KeyIdentity.generate(new Uint8Array(32).fill(42))
      const walletAgent = await HttpAgent.create({
        host: data.input.gateway,
        identity: walletIdentity,
        shouldFetchRootKey: false,
        verifyQuerySignatures: true
      })
      await walletAgent.fetchRootKey()
      const commerce = Actor.createActor<CommerceService>(commerceIdl, {
        agent: walletAgent,
        canisterId: data.input.commerce
      })
      const membership = Actor.createActor<MembershipService>(membershipIdl, {
        agent: walletAgent,
        canisterId: data.input.membership
      })
      const product = new CommerceClient(
        client,
        commerce,
        membership,
        data.input.commerce,
        data.input.membership,
        walletIdentity.getPrincipal()
      )
      const wallet = new WalletClient(walletAgent, walletIdentity.getPrincipal(), data.crypto)
      if (payload.action === 'quote') {
        await product.entitlement(true)
        const request = await product.personal('plus', 'Cash')
        const job = await product.quote(
          request,
          location.origin,
          payload.asset === 'CkUsdt' ? data.input.ledger_usdt : data.input.ledger
        )
        const quote = product.terms(job) as import('@dmsg/sdk').CheckoutQuote
        return {
          id: job.id,
          asset: quote.asset.asset,
          amount: quote.cash.amount_atomic.toString(),
          feeReserve: quote.cash.fee_reserve_atomic.toString()
        }
      }
      if (payload.action === 'open') {
        let calls = 0
        const proxy = new Proxy(commerce, {
          get(target, key) {
            if (key === 'open_checkout')
              return async (...args: Parameters<CommerceService['open_checkout']>) => {
                const reply = await target.open_checkout(...args)
                calls++
                if ('Err' in reply) throw new Error(JSON.stringify(reply.Err))
                throw new Error('Injected lost merchant reply')
              }
            return target[key as keyof CommerceService]
          }
        })
        try {
          await new CommerceClient(
            client,
            proxy,
            membership,
            data.input.commerce,
            data.input.membership,
            walletIdentity.getPrincipal()
          ).approve(payload.id)
        } catch (e) {
          if (!String(e).includes('Injected lost merchant reply')) throw e
        }
        await data.crypto.lock()
        await data.crypto.call('unlock', data.password)
        const result = (await product.approve(payload.id)) as import('@dmsg/sdk').CheckoutView
        return { state: result.progress.status, calls }
      }
      if (payload.action === 'fund') {
        const order = (await product.status(payload.id)) as import('@dmsg/sdk').CheckoutView,
          ledger = Principal.fromUint8Array(order.quote.cash.ledger).toText(),
          before = await wallet.balance(ledger)
        const fault = Actor.createActor<{ lose_next_response(): Promise<void> }>(
          () => IDL.Service({ lose_next_response: IDL.Func([], [], []) }),
          { agent: walletAgent, canisterId: ledger }
        )
        await fault.lose_next_response()
        let lost = false
        try {
          await wallet.transferCheckout(order)
        } catch (e) {
          lost = String(e).includes('injected lost response')
        }
        if (!lost) throw new Error('Ledger did not lose committed response')
        await data.crypto.lock()
        await data.crypto.call('unlock', data.password)
        const block = await wallet.transferCheckout(order),
          after = await wallet.balance(ledger),
          funded = (await product.funding(
            payload.id,
            block.toString()
          )) as import('@dmsg/sdk').CheckoutView
        if (
          before - after !==
          order.quote.cash.amount_atomic +
            order.quote.cash.fee_reserve_atomic +
            order.quote.asset.network_fee_atomic
        )
          throw new Error('Wallet was charged more than once')
        return {
          state: funded.progress.status,
          block: block.toString(),
          lost,
          charged: (before - after).toString()
        }
      }
      if (payload.action === 'refund') {
        let refused = false
        try {
          await product.cancel(payload.id)
        } catch (error) {
          if (!String(error).includes('Forbidden')) throw error
          refused = true
        }
        const value = (await product.status(payload.id)) as import('@dmsg/sdk').CheckoutView
        return { state: value.progress.status, refused }
      }
      if (payload.action === 'advance-sns') return await product.approve(payload.id, true)
      if (payload.action === 'review-sns') {
        const terms = product.terms(
          await product.job(payload.id)
        ) as import('@dmsg/sdk').PandaApplicationTerms
        return {
          actor: Principal.fromUint8Array(terms.actor).toText(),
          version: terms.quote.policy.policy_version,
          neuron: hex(terms.neuron_id)
        }
      }
      if (payload.action === 'sns') {
        await product.entitlement(true)
        const request = await product.personal('plus', 'Panda'),
          job = await product.quote(request, location.origin, '4d'.repeat(32)),
          result = (await product.approve(job.id)) as import('@dmsg/sdk').PandaClaimView
        return {
          id: job.id,
          status: result.status,
          eligibility: result.eligibility,
          actor: walletIdentity.getPrincipal().toText(),
          account: data.account
        }
      }
      if (payload.action === 'entitlement') {
        const value = await product.entitlement(true)
        return { plan: value.plan_snapshot.plan_id, source: value.source_status }
      }
    }
    if (action === 'shared') {
      const legacy = data.input.legacy!
      const shared = new SharedMigrationClient(
        client,
        new CloudClient({ origin: data.input.relay, environment: 'local' }),
        data.account,
        {
          channels: [legacy.channel],
          identity: legacy.identity,
          cutover: '08'.repeat(32),
          rootKey: legacy.root_key_hex
        }
      )
      const oldIdentity = Ed25519KeyIdentity.generate(
        new Uint8Array(32).fill(payload.seed ?? 52)
      )
      const oldAgent = await HttpAgent.create({
        host: legacy.gateway,
        identity: oldIdentity,
        shouldFetchRootKey: false,
        verifyQuerySignatures: true
      })
      await oldAgent.fetchRootKey()
      const approval = async (request: any) => {
        const result = await collectSnapshot(
          oldAgent,
          legacy.channel,
          {
            Attestation: {
              digest: unhex(request.digest),
              expires_at: BigInt(request.expires_at)
            }
          },
          [legacy.channel]
        )
        return { proof: encodeLegacyProof(result.proofs[0]), expires_at: request.expires_at }
      }
      if (payload.action === 'prepare') {
        const authority = await collectSnapshot(
          oldAgent,
          legacy.channel,
          { ChannelAuthority: legacy.channel_id },
          [legacy.channel]
        )
        const authorities = await collectSnapshot(
          oldAgent,
          legacy.identity,
          { Authorities: null },
          [legacy.identity]
        )
        return await shared.prepare(
          {
            format: 'dmsg-legacy-shared-source/1',
            source: legacy.channel,
            identity: legacy.identity,
            cutover: '08'.repeat(32),
            input: {
              channel: legacy.channel_id,
              authority: encodeLegacyProof(authority.proofs[0]),
              authorities: authorities.proofs.map(encodeLegacyProof)
            }
          },
          payload.version ?? 1
        )
      }
      if (
        payload.action === 'propose' ||
        payload.action === 'propose-lost' ||
        payload.action === 'consent'
      ) {
        const manager = oldIdentity.getPrincipal().toText()
        let stage = 'manager challenge'
        try {
          const request = await shared.managerChallenge(payload.draft, manager)
          stage = 'old identity approval'
          const consent = await approval(request)
          if (payload.action === 'propose-lost') {
            stage = 'proposal submission'
            const post = shared.cloud.post.bind(shared.cloud)
            let lost = false
            shared.cloud.post = async (...args) => {
              const result = await post(...args)
              if (!lost && args[0].endsWith('/proposals')) {
                lost = true
                throw new Error('Legacy proposal response lost')
              }
              return result
            }
            await shared.propose(payload.draft, manager, consent).catch((error) => {
              if (error.message !== 'Legacy proposal response lost') throw error
            })
            if (!lost) throw new Error('Legacy proposal fault was not exercised')
            stage = 'worker restart'
            await data.crypto.lock()
            await data.crypto.call('unlock', data.password)
            stage = 'directory reconciliation'
            return await shared.view(
              sharedSourceKey(payload.draft.proposal.source, payload.draft.proposal.channel)
            )
          }
          return payload.action === 'propose'
            ? await shared.propose(payload.draft, manager, consent)
            : await shared.consent(payload.draft, manager, consent)
        } catch (error) {
          throw new Error(
            `${stage}: ${error instanceof Error ? error.message : String(error)}`
          )
        }
      }
      if (payload.action === 'directory') return await shared.view(payload.key)
      if (payload.action === 'commit') return await shared.commit(payload.view)
      if (payload.action === 'activate') return await shared.activate(payload.view)
      if (payload.action === 'claim') {
        const request = await shared.claimChallenge(
          payload.view,
          oldIdentity.getPrincipal().toText()
        )
        return await shared.claim(payload.view, request.claim, await approval(request))
      }
      if (payload.action === 'share') {
        const channelKey = `${legacy.channel}/channel/${legacy.channel_id}`
        const channel = await collectSnapshot(
          oldAgent,
          legacy.channel,
          { Channel: legacy.channel_id },
          [legacy.channel]
        )
        const messageRows = await collectSnapshot(
          oldAgent,
          legacy.channel,
          { Messages: legacy.channel_id },
          [legacy.channel]
        )
        const blob = IDL.Vec(IDL.Nat8),
          channelType = IDL.Record({
            id: IDL.Nat32,
            canister: IDL.Principal,
            dek: blob,
            message_start: IDL.Nat32,
            latest_message_id: IDL.Nat32
          })
        const channelValue = IDL.decode([channelType], channel.entries[0][1])[0]
        const objects: LegacyArchive['inventory']['objects'] = []
        const add = (key: string, kind: 'channel' | 'message', value: unknown) => {
          const bytes = legacyPack(observation(value))
          objects.push({
            key,
            kind,
            bytes,
            digest: legacyDigest(bytes),
            observedAt: Date.now(),
            trust: 'query_observation'
          })
        }
        add(channelKey, 'channel', channelValue)
        const messageType = IDL.Record({
          id: IDL.Nat32,
          kind: IDL.Nat8,
          payload: blob,
          created_by: IDL.Principal,
          created_at: IDL.Nat64,
          reply_to: IDL.Nat32
        })
        for (const [, bytes] of messageRows.entries) {
          const [id, values] = IDL.decode(
            [IDL.Nat32, IDL.Opt(messageType)],
            bytes
          ) as unknown as [number, unknown[]]
          if (values.length) add(`${channelKey}/message/${id}`, 'message', values[0])
        }
        const principal = oldIdentity.getPrincipal().toText()
        const archive: LegacyArchive = {
          format: 'dmsg-legacy-archive/1',
          inventory: {
            format: 'dmsg-legacy-inventory/1',
            principal,
            messageCanister: legacy.channel,
            mode: 'Local',
            snapshot: 'pre_migration',
            objects,
            gaps: [],
            calls: []
          },
          keys: [
            {
              source: channelKey,
              principal,
              purpose: 'channel_kek',
              coseKey: unhex(legacy.kek_hex),
              parameters: legacyPack({})
            },
            {
              source: channelKey,
              principal,
              purpose: 'channel_dek',
              coseKey: unhex(legacy.dek_hex),
              parameters: legacyPack({})
            }
          ],
          checks: [],
          createdAt: Date.now()
        }
        const target = `chrome-extension://${location.hostname}`,
          pair = await data.crypto.call('legacyPair', 'https://dmsg.net', target)
        const sealed = await sealTransfer(encodeArchive(archive), pair.offer, {
          origin: 'https://dmsg.net',
          target,
          fingerprint: pair.fingerprint
        })
        const imported = await data.crypto.call('legacyImport', {
          file: new File([sealed], 'old-channel.dmsg-migration'),
          nonce: pair.offer.nonce,
          fingerprint: pair.fingerprint,
          principal
        })
        if (!imported.messages.some((m) => m.text === 'Legacy shared history 0'))
          throw new Error('Original historical COSE was not decoded')
        return await shared.shareHistory(payload.view, imported.key, payload.member)
      }
      if (payload.action === 'receive')
        return await shared.receiveHistory(
          payload.view,
          payload.grant,
          payload.recovery ? (payload.wrong ? '00'.repeat(32) : data.code) : undefined
        )
      if (payload.action === 'joined') return await shared.joined(payload.view)
    }
    if (action === 'channel') {
      const channel = new ChannelClient(
        client,
        new CloudClient({ origin: data.input.relay, environment: 'local' }),
        data.account
      )
      if (payload.action === 'offline')
        return {
          channels: await data.crypto.call('channelList'),
          messages: await data.crypto.call('channelMessages', payload.id),
          files: (await data.crypto.call('view')).entries
            .filter((e) => e.item.file)
            .map((e) => e.item.title)
        }
      if (payload.action === 'create') {
        const id = await channel.create('Real collaborative channel', 'collaboration')
        await channel.rotate(id)
        await channel.send(id, 'owner history before invitation')
        return { id, account: data.account }
      }
      if (payload.action === 'file') {
        const record = await data.crypto.call(
          'importFile',
          new File([new Uint8Array(1024 * 1024 + 17).fill(73)], 'channel-history.bin', {
            type: 'application/octet-stream'
          })
        )
        return await channel.sendFile(
          payload.id,
          record.key,
          undefined,
          'historical attachment'
        )
      }
      if (payload.action === 'download') {
        const file = await channel.downloadFile(payload.id, payload.seq),
          bytes = new Uint8Array(await file.blob.arrayBuffer())
        if (bytes.length !== 1024 * 1024 + 17 || !bytes.every((b) => b === 73))
          throw new Error('Shared file did not round-trip')
        return { name: file.name, size: bytes.length }
      }
      if (payload.action === 'invite') return await channel.invite(payload.id, payload.account)
      if (payload.action === 'join') return await channel.join(payload.invitation)
      if (payload.action === 'rotate') {
        const fetcher = globalThis.fetch
        let lost = false
        if (payload.lose)
          globalThis.fetch = async (...args) => {
            const response = await fetcher(...args)
            if (
              !lost &&
              response.ok &&
              new URL(String(args[0])).pathname.endsWith('/rotation/activate')
            ) {
              lost = true
              throw new Error('Injected lost epoch activation')
            }
            return response
          }
        try {
          await channel.rotate(payload.id)
        } catch (error) {
          if (!String(error).includes('Injected lost epoch activation')) throw error
        } finally {
          globalThis.fetch = fetcher
        }
        if (lost) {
          await data.crypto.lock()
          await data.crypto.call('unlock', data.password)
          await channel.rotate(payload.id)
        }
        return { ...(await channel.known(payload.id)), lost }
      }
      if (payload.action === 'send')
        return await channel.send(payload.id, payload.text, payload.messageId)
      if (payload.action === 'sync') return await channel.sync(payload.id)
      if (payload.action === 'history')
        return await channel.grantHistory(
          payload.id,
          payload.account,
          payload.from,
          payload.to
        )
      if (payload.action === 'backfill') return await channel.sync(payload.id, true)
      if (payload.action === 'sponsor-propose')
        return await channel.prepareSponsor(payload.id, payload.account)
      if (payload.action === 'sponsor-accept')
        return await channel.acceptSponsor(payload.packet)
      if (payload.action === 'sponsor-commit')
        return await channel.commitSponsor(payload.packet)
      if (payload.action === 'owner-propose')
        return await channel.prepareOwnerTransfer(payload.id, payload.account)
      if (payload.action === 'owner-accept')
        return await channel.acceptOwnerTransfer(payload.packet)
      if (payload.action === 'owner-commit')
        return await channel.commitOwnerTransfer(payload.packet)
      if (payload.action === 'remove')
        return await channel.control(payload.id, { type: 'remove', account: payload.account })
    }
    if (action === 'handle') {
      const registry = Actor.createActor<HandleService>(handleIdl, {
        agent: client.agent,
        canisterId: data.input.handle
      })
      let calls = 0
      const proxy = new Proxy(registry, {
        get(target, key) {
          if (key === 'claim_legacy_handle')
            return async (...args: Parameters<HandleService['claim_legacy_handle']>) => {
              const result = await target.claim_legacy_handle(...args)
              calls++
              if ('Err' in result) throw new Error(JSON.stringify(result.Err))
              throw new Error('Injected lost claim response')
            }
          return target[key as keyof HandleService]
        }
      })
      const handle = new HandleClient(client, proxy, data.input.handle, client.caller)
      const preview = await handle.prepare('legacy_probe')
      try {
        await handle.run()
        throw new Error('Expected lost claim response')
      } catch (error) {
        if (!String(error).includes('Injected lost claim response')) throw error
      }
      await data.crypto.lock()
      await data.crypto.call('unlock', data.password)
      const resumed = await new HandleClient(
        client,
        proxy,
        data.input.handle,
        client.caller
      ).run()
      if (resumed.phase !== 'claimed' || calls !== 1)
        throw new Error('Claim did not reconcile original operation')
      const ownership = await handle.ownership('legacy_probe')
      return {
        phase: resumed.phase,
        calls,
        account: ownership ? hex(ownership.owner_account) : '',
        snapshotCount: String(preview.snapshot.count)
      }
    }
    if (action === 'sign') {
      const state = await client.refresh(data.account)
      await client.mutate(data.account, {
        SetDeviceCapabilities: {
          device_id: unhex(client.meta.deviceId),
          capabilities: [...state.device!.input.capabilities, { FormalApprove: null }]
        }
      })
      const source = {
        origin: location.origin,
        tabId: 1,
        frameId: 0,
        documentId: 'signing-fixture'
      }
      const payload: SignatureRequest = {
        protocol: 'dmsg-extension/4',
        method: 'signature.request',
        requestId: id(),
        accountId: data.account,
        nonce: id(),
        expiresAt: String(Date.now() + 240000),
        statement: {
          issuer: state.info.issuer,
          content: { kind: 'text', text: 'Exact statement for real Wasm signing.' }
        }
      }
      const parsed = parseRequest(payload, source)
      const request: PendingRequest = {
        kind: 'document',
        id: payload.requestId,
        source,
        digest: parsed.digest,
        expiresAt: parsed.expiresAt,
        createdAt: Date.now(),
        state: 'awaiting_user',
        payload: await hpkeSeal(client.meta.hpkePublic, canonical(payload), [
          'dmsg/external-request/1',
          client.meta.subjectId,
          client.meta.deviceId,
          payload.requestId
        ])
      }
      const db = await WorkspaceDB.open((await currentWorkspace())!)
      await db.db.put('requests', request)
      db.db.close()
      const cose = Actor.createActor<CoseService>(coseIdl, {
        agent: client.agent,
        canisterId: data.input.cose
      })
      let dispatches = 0,
        live = true,
        signingError = ''
      const proxy = new Proxy(client.user, {
        get(target, key) {
          if (key === 'sign')
            return async (...args: Parameters<_SERVICE['sign']>) => {
              const result = await target.sign(...args)
              dispatches++
              if ('Err' in result) signingError = JSON.stringify(result.Err)
              throw new Error('Injected lost signing response')
            }
          return target[key as keyof _SERVICE]
        }
      })
      const lossy = new AccountClient(
        proxy,
        client.agent,
        client.caller,
        client.crypto,
        client.meta,
        client.home.toText()
      )
      const check = async () => {
        if (!live) throw new Error('Original source navigated')
      }
      const signing = new SigningClient(lossy, cose, check)
      await signing.prepare(request, payload)
      live = false
      let navigationRejected = false
      try {
        await signing.execute()
      } catch (error) {
        navigationRejected = String(error).includes('Original source navigated')
      }
      if (!navigationRejected || dispatches)
        throw new Error('Navigated source dispatched a signature')
      live = true
      try {
        await signing.execute()
        throw new Error('Expected lost signing response')
      } catch (error) {
        if (!String(error).includes('执行结果尚未确认')) throw error
      }
      if (signingError) throw new Error('Actual signing rejected: ' + signingError)
      await data.crypto.lock()
      await data.crypto.call('unlock', data.password)
      live = false // Reconciliation of a committed execution does not need the old page.
      const resumed = await new SigningClient(lossy, cose, check).resume(request.id)
      const repeat = await new SigningClient(lossy, cose, check).resume(request.id)
      if (
        resumed.stage !== 'complete' ||
        repeat.executionId !== resumed.executionId ||
        dispatches !== 1
      )
        throw new Error('Signing did not reconcile exactly once')
      return {
        stage: resumed.stage,
        executionId: resumed.executionId,
        dispatches,
        navigationRejected,
        artifact: !!resumed.artifact,
        receipt: !!resumed.receipt
      }
    }
    if (action === 'content') {
      const content = new ContentClient(
        client,
        new CloudClient({ origin: data.input.relay, environment: 'local' }),
        data.account
      )
      if (payload?.backgroundRestore) {
        const entry = (await data.crypto.call('view')).entries.find(
          (e) => e.item.title === 'Account conversion note'
        )!
        const deleted = await data.crypto.call('deleteItem', {
          id: entry.record.id,
          base: entry.record.revision
        })
        await content.push(deleted.key)
        const restored = await data.crypto.call('deleteItem', {
          id: deleted.id,
          base: deleted.revision,
          restore: true
        })
        await content.prepareBackground()
        await data.crypto.lock()
        const db = await WorkspaceDB.open((await currentWorkspace())!)
        try {
          await flushCipherDispatches(db, data.input.relay)
          const jobs = await db.db.getAll('meta')
          if (
            !jobs.some(
              (row: any) =>
                row.id.startsWith('dispatch:') &&
                row.value.revision === restored.revision &&
                row.value.state === 'complete'
            )
          )
            throw new Error('Locked background restore did not commit')
        } finally {
          db.db.close()
        }
        await data.crypto.call('unlock', data.password)
        await content.push(restored.key)
      }
      if (
        payload?.file &&
        !(await data.crypto.call('view')).entries.some(
          (e) => e.item.title === 'cloud-one-mib.bin'
        )
      )
        await data.crypto.call(
          'importFile',
          new File([new Uint8Array(1024 * 1024).fill(37)], 'cloud-one-mib.bin')
        )
      if (payload?.edit) {
        const entry = (await data.crypto.call('view')).entries.find(
          (e) => e.item.title === 'Account conversion note'
        )!
        await data.crypto.call('saveItem', {
          id: entry.record.id,
          base: entry.record.revision,
          item: { ...entry.item, body: payload.edit, updatedAt: Date.now() }
        })
      }
      if (payload?.prepare || payload?.expirePlan) {
        const title = payload?.expirePlan ? 'cloud-one-mib.bin' : 'Account conversion note'
        const entry = (await data.crypto.call('view')).entries.find(
          (e) => e.item.title === title
        )!
        const job = await data.crypto.call('contentPrepare', entry.record.key)
        if (payload?.expirePlan) {
          for (const upload of job.uploads) upload.plan.expires_at = Date.now() - 1
          await data.crypto.call('contentSave', job)
        }
      }
      const fetcher = globalThis.fetch,
        lost = new Set<string>()
      if (payload?.lose)
        globalThis.fetch = async (...args) => {
          const response = await fetcher(...args),
            path = new URL(String(args[0])).pathname
          if (
            args[1]?.method === 'POST' &&
            response.ok &&
            ['/uploads', '/uploads/finalize', '/vault'].some((s) => path.endsWith(s)) &&
            !lost.has(path)
          ) {
            lost.add(path)
            throw new Error('Injected content reply loss')
          }
          return response
        }
      try {
        if (payload?.push) {
          for (let attempt = 0; ; attempt++) {
            try {
              await content.pushPending()
              break
            } catch (error) {
              if (
                !(
                  error instanceof Error &&
                  error.message.includes('Injected content reply loss')
                ) ||
                attempt >= 3
              )
                throw error
              await data.crypto.lock()
              await data.crypto.call('unlock', data.password)
            }
          }
        }
        const pulled = await content.pull(),
          view = await data.crypto.call('view')
        const entry = view.entries.find((e) => e.item.title === 'cloud-one-mib.bin')
        if (entry) {
          const result = await data.crypto.call('downloadFile', entry.record.key)
          const bytes = new Uint8Array(await result.blob.arrayBuffer())
          if (bytes.length !== 1024 * 1024 || !bytes.every((b) => b === 37))
            throw new Error('Cloud file did not round-trip')
        }
        const backup = await data.crypto.call('exportBackup', data.password)
        data.backup = await backup.blob.text()
        return {
          ...pulled,
          lost: lost.size,
          entries: view.entries.length,
          conflicts: view.conflicts.length,
          file: !!entry,
          note: view.entries.find((e) => e.item.title === 'Account conversion note')?.item.body
        }
      } finally {
        globalThis.fetch = fetcher
      }
    }
    if (action === 'export-local') {
      const result = await data.crypto.call('exportBackup', data.password)
      data.backup = await result.blob.text()
      return {
        input: data.input,
        account: data.account,
        code: data.code,
        backup: data.backup,
        missing: result.missing
      }
    }
    if (action === 'capture-evidence') {
      const raw = xidBytes(data.account)
      oldEvidence = {
        info: await client.user.get_account(raw),
        batch: await client.user.security_snapshot_batch([raw]),
        bundle: await client.user.get_device_bundle(raw)
      }
      return true
    }
    if (action === 'stale-evidence') {
      if (!oldEvidence) throw new Error('Missing captured evidence')
      const proxy = new Proxy(client.user, {
        get(target, key) {
          if (key === 'get_account') return async () => oldEvidence!.info
          if (key === 'security_snapshot_batch') return async () => oldEvidence!.batch
          if (key === 'get_device_bundle') return async () => oldEvidence!.bundle
          return target[key as keyof _SERVICE]
        }
      })
      return await new AccountClient(
        proxy,
        client.agent,
        client.caller,
        client.crypto,
        client.meta,
        client.home.toText()
      ).refresh(data.account)
    }
    if (action === 'request-recovery') {
      const r = await client.requestRecovery(data.account, data.code)
      return { executeAfter: Number(r.pending!.execute_after) }
    }
    if (action === 'dispute') {
      const r = await client.recoveryStatus(data.account)
      await client.mutate(data.account, {
        DisputeRecovery: {
          op_id: r.pending!.request.op_id,
          dispute: new Uint8Array(32).fill(9)
        }
      })
      return true
    }
    if (action === 'reconfirm') {
      const r = await client.reconfirmRecovery(data.account, data.code)
      return {
        reconfirmed: r.pending!.reconfirmed,
        executeAfter: Number(r.pending!.execute_after)
      }
    }
    if (action === 'complete') {
      const r = await client.completeRecovery(data.account)
      return {
        device: hex(Uint8Array.from(r.device!.input.device_id)),
        rekey: 'RekeyRequired' in r.info.vault_write_state,
        bindings: r.info.auth_bindings.map((p) => p.toText())
      }
    }
    if (action === 'check-authority') {
      await client.refresh(data.account)
      return true
    }
    if (action === 'pair')
      return await client.pairing(
        data.account,
        payload === 'Administrator' ? 'Administrator' : 'Member'
      )
    if (action === 'approve') {
      await client.approvePairing(data.account, payload)
      return true
    }
    if (action === 'revoke') {
      await client.mutate(data.account, { RevokeDevice: { device_id: unhex(payload) } })
      return true
    }
    if (action === 'rotate' || action === 'open') {
      const root = new AccountRootClient(
        client,
        new CloudClient({ origin: data.input.relay, environment: 'local' })
      )
      const job =
        action === 'rotate'
          ? await root.rotate(data.account)
          : await root.openCurrent(data.account)
      const state = await client.refresh(data.account)
      await data.crypto.call('activateAccountRoot', {
        context: job.context!,
        digest: hash(unb64(job.bytes!)),
        uploadId: String(job.plan!.upload_id),
        homeUser: data.input.user,
        issuer: state.info.issuer,
        password: data.password
      })
      await data.crypto.call('saveItem', {
        item: {
          type: 'note',
          title: `Root ${job.context!.generation}`,
          body: 'new generation content',
          username: '',
          secret: '',
          url: '',
          tags: [],
          favorite: false,
          createdAt: 2,
          updatedAt: 2
        }
      })
      const exported = await data.crypto.call('exportBackup', data.password)
      data.backup = await exported.blob.text()
      return {
        generation: job.context!.generation,
        entries: (await data.crypto.call('view')).entries.length
      }
    }
    throw new Error('Unknown test action')
  } finally {
    clearInterval(timer)
    await data.crypto.lock()
  }
}
;(window as any).accountProbe = async (input: ProbeInput) => {
  authSeed = input.identitySeed ?? 42
  const crypto = new CryptoClient(),
    password = 'integration-only-local-password'
  const timer = setInterval(() => void crypto.call('tick').catch(() => {}), 5000)
  try {
    const initialized = await crypto.call('initialize', password)
    await crypto.call('verifyRecovery', initialized.recoveryCode)
    await crypto.call('saveItem', {
      item: {
        type: 'note',
        title: 'Account conversion note',
        body: 'local private content',
        username: '',
        secret: '',
        url: '',
        tags: [],
        favorite: false,
        createdAt: 1,
        updatedAt: 1
      }
    })
    await crypto.call('importFile', new File([new Uint8Array([1, 2, 3, 4])], 'test.bin'))
    const original = await currentWorkspace()
    const identity = Ed25519KeyIdentity.generate(new Uint8Array(32).fill(authSeed))
    const agent = await HttpAgent.create({
      host: input.gateway,
      identity,
      shouldFetchRootKey: false,
      verifyQuerySignatures: true
    })
    await agent.fetchRootKey()
    const actor = Actor.createActor<_SERVICE>(idlFactory, { agent, canisterId: input.user })
    let loseCreate = true
    const proxy = new Proxy(actor, {
      get(target, key) {
        if (key === 'create_account')
          return async (...args: Parameters<_SERVICE['create_account']>) => {
            const result = await target.create_account(...args)
            if (loseCreate) {
              loseCreate = false
              throw new Error('Injected lost reply after commit')
            }
            return result
          }
        return target[key as keyof _SERVICE]
      }
    })
    let client = new AccountClient(
      proxy,
      agent,
      identity.getPrincipal(),
      crypto,
      initialized.meta,
      input.user
    )
    try {
      await client.create()
      throw new Error('Expected lost response')
    } catch (e) {
      if (!(e as Error).message.includes('响应尚未确认')) throw e
    }
    // Terminate the real dedicated worker and reopen the encrypted journal.
    await crypto.lock()
    await crypto.call('unlock', password)
    client = new AccountClient(
      actor,
      agent,
      identity.getPrincipal(),
      crypto,
      initialized.meta,
      input.user
    )
    const account = (await client.resume())!
    const own = await client.refresh(account)
    if (!own.device || own.device.revoked_at.length)
      throw new Error('Initial device not approved')
    const outsider = new AccountClient(
      actor,
      agent,
      identity.getPrincipal(),
      crypto,
      { ...initialized.meta, deviceId: '11'.repeat(32) },
      input.user
    )
    if ((await outsider.refresh(await outsider.create())).device)
      throw new Error('Second device was implicitly approved')
    const recovery = await crypto.call('accountRecovery', {
      account,
      generation: 1,
      action: 'generate'
    })
    await client.enrollRecovery(account, recovery.code, {
      generation: 1n,
      delay_ms: 86400000n,
      signing_pub: unb64(recovery.signingPublic),
      hpke_pub: unb64(recovery.hpkePublic)
    })
    const extraIdentity = Ed25519KeyIdentity.generate(new Uint8Array(32).fill(authSeed + 2))
    const extraAgent = await HttpAgent.create({
      host: input.gateway,
      identity: extraIdentity,
      shouldFetchRootKey: false,
      verifyQuerySignatures: true
    })
    await extraAgent.fetchRootKey()
    const extraActor = Actor.createActor<_SERVICE>(idlFactory, {
      agent: extraAgent,
      canisterId: input.user
    })
    const extraClient = new AccountClient(
      extraActor,
      extraAgent,
      extraIdentity.getPrincipal(),
      crypto,
      initialized.meta,
      input.user
    )
    const binding = await extraClient.beginBinding(account)
    await client.approveBinding(account, binding)
    if (
      !(await client.refresh(account)).info.auth_bindings.some(
        (p) => p.toText() === extraIdentity.getPrincipal().toText()
      )
    )
      throw new Error('Authentication binding was not committed')
    const cloud = new CloudClient({ origin: input.relay, environment: 'local' })
    let loseDerive = true,
      loseCommit = true
    const rootActor = new Proxy(actor, {
      get(target, key) {
        if (key === 'derive_root')
          return async (...args: Parameters<_SERVICE['derive_root']>) => {
            const result = await target.derive_root(...args)
            if (loseDerive) {
              loseDerive = false
              throw new Error('Injected lost derivation reply')
            }
            return result
          }
        if (key === 'mutate_account')
          return async (...args: Parameters<_SERVICE['mutate_account']>) => {
            const result = await target.mutate_account(...args)
            if ('CommitRoot' in args[0].command && loseCommit) {
              loseCommit = false
              throw new Error('Injected lost commitment reply')
            }
            return result
          }
        return target[key as keyof _SERVICE]
      }
    })
    client = new AccountClient(
      rootActor,
      agent,
      identity.getPrincipal(),
      crypto,
      initialized.meta,
      input.user
    )
    const root = new AccountRootClient(client, cloud)
    let interruptions = 0,
      job: Awaited<ReturnType<AccountRootClient['run']>> | undefined
    for (let i = 0; i < 3; i++) {
      try {
        job = await root.run(account)
        break
      } catch (error) {
        if (
          !(error as Error).message.includes('Injected lost derivation') &&
          !(error as Error).message.includes('响应尚未确认')
        )
          throw error
        interruptions++
        await crypto.lock()
        await crypto.call('unlock', password)
      }
    }
    if (!job || interruptions !== 2 || (await client.pending()))
      throw new Error('Root interruptions were not reconciled')
    const committed = await client.refresh(account)
    if (
      !job.bytes ||
      !job.context ||
      hash(unb64(job.bytes)) !==
        hex(Uint8Array.from(committed.info.current_root[0]!.bundle_digest))
    )
      throw new Error('Root mismatch')
    const meta = await crypto.call('activateAccountRoot', {
      context: job.context,
      digest: hash(unb64(job.bytes)),
      uploadId: String(job.plan!.upload_id),
      homeUser: input.user,
      issuer: committed.info.issuer,
      password
    })
    const view = await crypto.call('view')
    if (meta.subjectId !== account || view.entries.length !== 2)
      throw new Error('Conversion missing content')
    if (!view.entries.every((e) => e.record.subjectId === account))
      throw new Error('Conversion retained fake identity')
    const reg = await registry(),
      records = await reg.getAll('workspaces')
    reg.close()
    if (!records.some((r) => r.name === original && !r.active && r.retained))
      throw new Error('Original source was not retained')
    const backup = await crypto.call('exportBackup', password)
    const archive = JSON.parse(await backup.blob.text())
    if (JSON.stringify(archive).includes(recovery.code.replaceAll('-', '')))
      throw new Error('Recovery secret in export')
    await crypto.lock()
    await crypto.call('unlock', password)
    if ((await crypto.call('view')).entries.length !== 2)
      throw new Error('Restart lost converted content')
    const source = await WorkspaceDB.open(original!)
    if ((await source.db.getAll('objects')).length !== 2) throw new Error('Source changed')
    source.db.close()
    saved = {
      input,
      crypto,
      password,
      account,
      code: recovery.code,
      backup: await backup.blob.text()
    }
    return {
      deriveCost: String(
        (decodeControlResult('derive_root', job.result!) as any).Ok.cycles_cost_upper_bound
      ),
      account,
      device: meta.deviceId,
      rootCommitted: job.stage === 'committed',
      converted: true,
      lostReplyResumed: true,
      concurrentCreateBlocked: true,
      rootDigest: hash(unb64(job.bytes)),
      generation: job.context.generation,
      backupBytes: backup.blob.size
    }
  } finally {
    clearInterval(timer)
    await crypto.lock()
  }
}
