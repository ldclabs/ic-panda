<script lang="ts">
  import type { SvelteComponent } from 'svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconIcLogo from '$lib/components/icons/IconIcLogo.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import { getModalStore, clipboard, popup } from '@skeletonlabs/skeleton'
  import type { PopupSettings } from '@skeletonlabs/skeleton'
  import { authStore } from '$lib/stores/auth'
  import IconCopy from '../icons/IconCopy.svelte'
  import { signOut } from '$lib/services/auth'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent

  const modalStore = getModalStore()

  function onLogoutHandler(): void {
    signOut().then(() => {
      modalStore.close()
    })
  }

  function shortId(id: string): string {
    return id.length > 14 ? id.slice(0, 7) + '...' + id.slice(-7) : id
  }

  const principalHover: PopupSettings = {
    event: 'hover',
    target: 'principalHover',
    placement: 'bottom'
  }

  let copiedClass = ''

  function onCopyHandler(): void {
    copiedClass = 'text-secondary-500'
    setTimeout(() => {
      copiedClass = ''
    }, 5000)
  }

  let icp_balance = 0

  $: principal = $authStore.identity?.getPrincipal().toString() || '2vxsx-fae'
</script>

{#if $modalStore[0]}
  <!-- This is a hack to fix the focus issue -->
  <button class="hidden"></button>
  <div class="card w-modal relative mt-10 space-y-4 bg-white/95 p-4 shadow-xl">
    <button
      class="z-1 variant-filled-surface btn btn-icon btn-icon-sm absolute -right-4 -top-4 hover:scale-110"
      on:click={parent['onClose']}
    >
      <IconClose />
    </button>
    <header class="!mt-0 text-center text-xl font-bold">Account</header>
    <div class="flex flex-row gap-2">
      <span>Your Principal:</span>
      <span class={copiedClass} use:popup={principalHover}>
        <strong>{shortId(principal)}</strong>
      </span>
      <button
        class={copiedClass}
        use:clipboard={principal}
        on:click={onCopyHandler}
        disabled={copiedClass != ''}
      >
        <IconCopy />
      </button>
      <div class="card variant-filled-primary p-2" data-popup="principalHover">
        <p>{principal}</p>
        <div class="variant-filled-primary arrow" />
      </div>
    </div>
    <ul
      class="list !mt-10 space-y-4 *:flex *:h-10 *:flex-row *:justify-between *:!rounded-md *:px-4 *:py-2"
    >
      <li
        class="*:flex *:flex-row *:content-center *:justify-center *:gap-3 hover:bg-primary-100/50"
      >
        <div class="leading-8">
          <span class="*:size-8"><IconIcLogo /></span>
          <span>Internet Computer</span>
        </div>
        <div class="leading-8 *:min-w-14">
          <span class="text-right">{icp_balance}</span>
          <span>ICP</span>
        </div>
      </li>
      <li
        class="*:flex *:flex-row *:content-center *:justify-center *:gap-3 hover:bg-primary-100/50"
      >
        <div class="leading-8">
          <span class="*:size-8"><IconPanda /></span>
          <span>ICPanda</span>
        </div>
        <div class="leading-8 *:min-w-14">
          <span class="text-right">{icp_balance}</span>
          <span>PANDA</span>
        </div>
      </li>
    </ul>
    <footer class="!mt-16 {parent['regionFooter']}">
      <button
        class="variant-filled-surface btn btn-sm"
        on:click={onLogoutHandler}
      >
        <IconLogout />
        <span>Logout</span>
      </button>
    </footer>
  </div>
{/if}
