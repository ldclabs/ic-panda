import { agent, anonAgent } from '$lib/utils/auth'
import { Actor, type ActorMethod, type ActorSubclass } from '@dfinity/agent'
import type { IDL } from '@dfinity/candid'
import type { Principal } from '@dfinity/principal'

export function createActor<T = Record<string, ActorMethod>>({
  canisterId,
  idlFactory
}: {
  canisterId: string | Principal
  idlFactory: IDL.InterfaceFactory
}): ActorSubclass<T> {
  // Creates an actor with using the candid interface and the HttpAgent
  return Actor.createActor(idlFactory, {
    // queryTransform: (methodName: string, args: unknown[], callConfig: any) => {
    //   console.log('queryTransform', methodName, args, callConfig)
    // },
    agent,
    canisterId
  })
}

export function createAnonActor<T = Record<string, ActorMethod>>({
  canisterId,
  idlFactory
}: {
  canisterId: string | Principal
  idlFactory: IDL.InterfaceFactory
}): ActorSubclass<T> {
  // Creates an actor with using the candid interface and the HttpAgent
  return Actor.createActor(idlFactory, {
    // queryTransform: (methodName: string, args: unknown[], callConfig: any) => {
    //   console.log('queryTransform', methodName, args, callConfig)
    // },
    agent: anonAgent,
    canisterId
  })
}
