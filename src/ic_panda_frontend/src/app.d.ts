// See https://kit.svelte.dev/docs/types#app
// for information about these interfaces
declare global {
  namespace App {
    interface Error {
      code: string
      id: string
    }
    // interface Locals {}
    // interface PageData {}
    // interface Platform {}
  }
}

/* eslint-disable */

declare namespace svelteHTML {
  interface HTMLAttributes<T> {
    'on:pandaTriggerWallet'?: (event: CustomEvent<any>) => void
  }
}

declare module '@lottiefiles/svelte-lottie-player' {
  import type { SvelteComponentTyped } from 'svelte'

  export class LottiePlayer extends SvelteComponentTyped<{
    autoplay?: boolean
    background: string
    controls: boolean
    controlsLayout?: string[]
    count?: number
    defaultFrame?: number
    direction?: number
    height: number
    hover?: boolean
    loop?: boolean
    mode?: 'normal' | 'bounce'
    onToggleZoom?: (boolean) => void
    renderer?: 'svg' | 'canvas'
    speed?: number
    src?: string
    style?: string
    width: number
  }> {}
}
