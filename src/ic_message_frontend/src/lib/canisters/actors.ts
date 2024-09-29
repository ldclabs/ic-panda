import { agent } from '$lib/stores/auth'
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
    agent,
    canisterId
    // queryTransform: (methodName: string, args: unknown[], callConfig: any) => {
    //   console.log('queryTransform', methodName, args, callConfig)
    // }
  })
}
