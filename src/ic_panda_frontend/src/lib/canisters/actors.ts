import { IS_LOCAL } from '$lib/constants'
import {
  Actor,
  type ActorMethod,
  type ActorSubclass,
  type HttpAgent,
  type Identity
} from '@dfinity/agent'
import type { IDL } from '@dfinity/candid'
import type { Principal } from '@dfinity/principal'
import { createAgent as createAgentUtils } from '@dfinity/utils'

let agents: Record<string, HttpAgent> = {}

export const createActor = async <T = Record<string, ActorMethod>>({
  canisterId,
  idlFactory,
  identity
}: {
  canisterId: string | Principal
  idlFactory: IDL.InterfaceFactory
  identity: Identity
}): Promise<ActorSubclass<T>> => {
  const agent = await getAgent({ identity })

  // Creates an actor with using the candid interface and the HttpAgent
  return Actor.createActor(idlFactory, {
    agent,
    canisterId
  })
}

export const getAgent = async ({
  identity
}: {
  identity: Identity
}): Promise<HttpAgent> => {
  const key = identity.getPrincipal().toText()

  if (!agents[key]) {
    agents[key] = await createAgent({ identity })
  }

  return agents[key] as HttpAgent
}

export const clearAgents = () => (agents = {})

const createAgent = ({
  identity
}: {
  identity: Identity
}): Promise<HttpAgent> =>
  createAgentUtils({
    identity,
    fetchRootKey: IS_LOCAL,
    host: IS_LOCAL ? 'http://localhost:4943/' : 'https://icp-api.io',
    verifyQuerySignatures: false
  })
