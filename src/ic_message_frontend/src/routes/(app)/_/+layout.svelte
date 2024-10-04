<script lang="ts">
  import { page } from '$app/stores'
  import { authStore } from '$lib/stores/auth'
  import IconUser from '$lib/components/icons/IconUser1.svelte'
  import IconMessage3Line from '$lib/components/icons/IconMessage3Line.svelte'
  import IconMoreFill from '$lib/components/icons/IconMoreFill.svelte'
  import { initPopup } from '$lib/utils/popup'
  import { onDestroy } from 'svelte'
  import MoreMenuPopup from '$lib/components/core/MoreMenuPopup.svelte'

  $: principal = $authStore.identity.getPrincipal()
  $: selectedProfile = selected('Profile', $page.url?.pathname || '')
  $: selectedMessages = selected('Messages', $page.url?.pathname || '')

  const { popupOpenOn, popupDestroy } = initPopup({
    target: 'popupNavigationMore'
  })

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

  onDestroy(() => {
    popupDestroy()
  })
</script>

{#key principal.toText()}
  <div
    class="mx-auto grid h-dvh w-full bg-white shadow-md max-md:grid-rows-[1fr_auto] md:max-w-5xl md:grid-cols-[auto_1fr]"
  >
    <slot />
    <div
      class="nav grid items-start gap-2 border-surface-500/20 bg-gray-100/50 *:flex *:flex-col *:items-center *:justify-center *:py-1 *:text-xs max-md:h-[60px] max-md:grid-cols-3 max-md:border-t md:order-first md:grid-rows-[auto_1fr_auto] md:border-r md:p-2"
    >
      <a
        href="/_/profile"
        role="button"
        class="transition-all {selectedProfile
          ? 'cursor-default text-primary-500'
          : 'hover:scale-105'}"
      >
        <span><IconUser /></span>
        <span>Profile</span>
      </a>
      <a
        href="/_/messages"
        role="button"
        class="transition-all {selectedMessages
          ? 'cursor-default text-primary-500'
          : 'hover:scale-105'}"
      >
        <span><IconMessage3Line /></span>

        <span>Messages</span>
      </a>
      <button
        class="btn px-0 transition-all hover:scale-105"
        on:click={(ev) => {
          popupOpenOn(ev.currentTarget)
        }}
      >
        <span><IconMoreFill /></span>
        <span class="!m-0">More</span>
      </button>
    </div>
  </div>
{/key}
<MoreMenuPopup target="popupNavigationMore" />

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
