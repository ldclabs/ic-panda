<script lang="ts">
  import { page } from '$app/state'
  import MoreMenuPopup from '$lib/components/core/MoreMenuPopup.svelte'
  import IconMessage3Line from '$lib/components/icons/IconMessage3Line.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import IconUser from '$lib/components/icons/IconUser1.svelte'
  import { authStore } from '$lib/stores/auth'
  import { type Snippet } from 'svelte'

  interface Props {
    children?: Snippet
  }

  let { children }: Props = $props()

  function selected(
    tab: 'Profile' | 'Messages' | 'More',
    pathname: string
  ): boolean {
    if (pathname.startsWith('/_')) {
      if (pathname.startsWith('/_/profile')) {
        return tab === 'Profile'
      } else {
        return tab === 'Messages'
      }
    }
    return tab === 'More'
  }

  let principal = $derived($authStore.identity.getPrincipal())
  let selectedProfile = $derived(selected('Profile', page.url?.pathname || ''))
  let selectedMessages = $derived(
    selected('Messages', page.url?.pathname || '')
  )

  const children_render = $derived(children)
</script>

{#key principal.toText()}
  <div
    class="mx-auto grid h-dvh w-full bg-white shadow-md max-md:grid-rows-[1fr_auto] md:max-w-5xl md:grid-cols-[auto_1fr] dark:bg-neutral-900"
  >
    {@render children_render?.()}
    <div
      class="nav border-surface-500/20 grid items-start gap-2 *:flex *:flex-col *:items-center *:justify-center *:py-1 *:text-xs max-md:h-[60px] max-md:grid-cols-3 max-md:border-t md:order-first md:grid-rows-[auto_1fr_auto] md:border-r md:p-2"
    >
      <a
        href="/_/profile"
        role="button"
        class="transition-all {selectedProfile
          ? 'text-primary-500 cursor-default'
          : 'hover:scale-105'}"
      >
        <span><IconUser /></span>
        <span>Profile</span>
      </a>
      <a
        href="/_/messages"
        role="button"
        class="transition-all {selectedMessages
          ? 'text-primary-500 cursor-default'
          : 'hover:scale-105'}"
      >
        <span><IconMessage3Line /></span>

        <span>Messages</span>
      </a>
      <MoreMenuPopup triggerClass="btn px-0 transition-all hover:scale-105">
        {#snippet trigger()}
          <span><IconMoreFill /></span>
          <span class="!m-0">More</span>
        {/snippet}
      </MoreMenuPopup>
    </div>
  </div>
{/key}

<style>
  @media (min-width: 768px) {
    .nav {
      background: linear-gradient(
        180deg,
        rgba(17, 194, 145, 0.1) -2%,
        rgba(255, 255, 255, 0) 35%
      );
    }
  }
</style>
