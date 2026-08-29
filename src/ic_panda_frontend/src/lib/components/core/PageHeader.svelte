<script lang="ts">
  import { page } from '$app/state'
  import AccountDetailModal from '$lib/components/core/AccountDetailModal.svelte'
  import IconArrowRightUp from '$lib/components/icons/IconArrowRightUp.svelte'
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconPanda from '$lib/components/icons/IconPanda.svelte'
  import IconUser0 from '$lib/components/icons/IconUser0.svelte'
  import IconUser1 from '$lib/components/icons/IconUser1.svelte'
  import IconX from '$lib/components/icons/IconX.svelte'
  import { signIn } from '$lib/services/auth'
  import { authStore } from '$lib/stores/auth'
  import { LINKS, NAV } from '$lib/site'
  import { getModalStore } from '$lib/ui/stores'

  const modalStore = getModalStore()

  let menuOpen = $state(false)

  const isApp = $derived(page.url?.pathname.startsWith('/_/') ?? false)
  const anonymous = $derived($authStore.identity.getPrincipal().isAnonymous())

  function isActive(href: string): boolean {
    return page.url?.pathname === '/' && '/' + page.url.hash === href
  }

  async function handleSignIn() {
    await signIn({})
  }

  function showAccountDetail() {
    modalStore.trigger({
      type: 'component',
      component: { ref: AccountDetailModal }
    })
  }

  function goHome(ev: MouseEvent) {
    menuOpen = false
    window.document.getElementById('page')?.scrollTo(0, 0)
    if (ev.detail == 2) {
      window.location.reload() // double click reloads, for standalone windows
    }
  }
</script>

<div class="border-b border-ink/15 bg-paper/85 backdrop-blur-md">
  <div
    class="mx-auto flex h-14 w-full max-w-6xl items-center gap-4 px-5 md:h-16 md:px-10"
  >
    <!-- Wordmark -->
    <a
      class="group flex shrink-0 items-center gap-2.5"
      href="/"
      onclick={goHome}
      title="ICPanda DAO"
    >
      <span
        class="shrink-0 overflow-hidden rounded-full ring-1 ring-panda transition-transform duration-300 *:size-10 group-hover:rotate-[8deg]"
      >
        <IconPanda />
      </span>
      <span class="font-mono text-xl font-semibold tracking-[0.02em]">
        ICPanda DAO
      </span>
    </a>

    <!-- Primary nav -->
    <nav class="ml-auto hidden items-center gap-7 md:flex">
      {#each NAV as item (item.href)}
        <a
          class="relative font-mono text-sm text-ink-70 transition-colors duration-200 hover:text-ink"
          class:!text-ink={isActive(item.href)}
          href={item.href}
          onclick={() => (menuOpen = false)}
        >
          {item.label}
          {#if isActive(item.href)}
            <span class="absolute -bottom-1.5 left-0 h-px w-full bg-ink"></span>
          {/if}
        </a>
      {/each}
    </nav>

    <div class="ml-auto flex items-center gap-1 md:ml-6 md:gap-2">
      <a
        class="hidden size-9 items-center justify-center text-ink-70 transition-colors hover:text-ink md:flex"
        href={LINKS.github}
        target="_blank"
        rel="noreferrer"
        title="GitHub"
        aria-label="GitHub"><IconGithub /></a
      >
      <a
        class="hidden size-9 items-center justify-center text-ink-70 transition-colors hover:text-ink md:flex"
        href={LINKS.x}
        target="_blank"
        rel="noreferrer"
        title="X"
        aria-label="X"><IconX /></a
      >

      {#if isApp}
        {#if anonymous}
          <button
            type="button"
            class="inline-flex items-center gap-2 border border-ink px-3 py-2 font-mono text-xs uppercase tracking-[0.08em] text-ink transition-colors hover:bg-ink hover:text-paper"
            onclick={handleSignIn}
          >
            <span class="*:size-4 max-md:hidden"><IconUser0 /></span>
            <span>Login</span>
          </button>
        {:else}
          <button
            type="button"
            class="flex size-9 items-center justify-center border border-ink/20 transition-colors hover:border-ink"
            onclick={showAccountDetail}
            aria-label="Account"
          >
            <span class="*:size-5"><IconUser1 /></span>
          </button>
        {/if}
      {/if}

      <button
        type="button"
        class="flex size-9 items-center justify-center text-ink md:hidden"
        onclick={() => (menuOpen = !menuOpen)}
        aria-expanded={menuOpen}
        aria-label="Menu"
      >
        {#if menuOpen}
          <span class="*:size-6"><IconClose /></span>
        {:else}
          <span class="flex w-5 flex-col gap-[5px]" aria-hidden="true">
            <span class="h-px w-full bg-ink"></span>
            <span class="h-px w-full bg-ink"></span>
            <span class="h-px w-3/5 bg-ink"></span>
          </span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Mobile menu -->
  {#if menuOpen}
    <nav class="border-t border-ink/15 bg-paper px-5 pb-4 pt-2 md:hidden">
      {#each NAV as item (item.href)}
        <a
          class="block border-b border-ink/10 py-3 font-mono text-sm"
          href={item.href}
          onclick={() => (menuOpen = false)}>{item.label}</a
        >
      {/each}
      <div class="flex items-center gap-6 pt-4">
        <a
          class="link-mark"
          href={LINKS.github}
          target="_blank"
          rel="noreferrer"
        >
          <span>GitHub</span>
          <span class="*:size-4"><IconArrowRightUp /></span>
        </a>
        <a class="link-mark" href={LINKS.x} target="_blank" rel="noreferrer">
          <span>X</span>
          <span class="*:size-4"><IconArrowRightUp /></span>
        </a>
      </div>
    </nav>
  {/if}
</div>
