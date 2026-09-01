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
    await signIn()
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

<div class="border-ink/15 bg-paper/85 border-b backdrop-blur-md">
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
        class="ring-panda shrink-0 overflow-hidden rounded-full ring-1 transition-transform duration-300 *:size-10 group-hover:rotate-[8deg]"
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
          class="text-ink-70 hover:text-ink relative font-mono text-sm transition-colors duration-200"
          class:!text-ink={isActive(item.href)}
          href={item.href}
          onclick={() => (menuOpen = false)}
        >
          {item.label}
          {#if isActive(item.href)}
            <span class="bg-ink absolute -bottom-1.5 left-0 h-px w-full"></span>
          {/if}
        </a>
      {/each}
    </nav>

    <div class="ml-auto flex items-center gap-1 md:ml-6 md:gap-2">
      <a
        class="text-ink-70 hover:text-ink hidden size-9 items-center justify-center transition-colors md:flex"
        href={LINKS.github}
        target="_blank"
        rel="noreferrer"
        title="GitHub"
        aria-label="GitHub"><IconGithub /></a
      >
      <a
        class="text-ink-70 hover:text-ink hidden size-9 items-center justify-center transition-colors md:flex"
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
            class="border-ink text-ink hover:bg-ink hover:text-paper inline-flex items-center gap-2 rounded-lg border px-3 py-2 font-mono text-xs tracking-[0.08em] uppercase transition-colors"
            onclick={handleSignIn}
          >
            <span class="*:size-4 max-md:hidden"><IconUser0 /></span>
            <span>Login</span>
          </button>
        {:else}
          <button
            type="button"
            class="border-ink/20 hover:border-ink flex size-9 items-center justify-center rounded-lg border transition-colors"
            onclick={showAccountDetail}
            aria-label="Account"
          >
            <span class="*:size-5"><IconUser1 /></span>
          </button>
        {/if}
      {/if}

      <button
        type="button"
        class="text-ink flex size-9 items-center justify-center md:hidden"
        onclick={() => (menuOpen = !menuOpen)}
        aria-expanded={menuOpen}
        aria-label="Menu"
      >
        {#if menuOpen}
          <span class="*:size-6"><IconClose /></span>
        {:else}
          <span class="flex w-5 flex-col gap-[5px]" aria-hidden="true">
            <span class="bg-ink h-px w-full"></span>
            <span class="bg-ink h-px w-full"></span>
            <span class="bg-ink h-px w-3/5"></span>
          </span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Mobile menu -->
  {#if menuOpen}
    <nav class="border-ink/15 bg-paper border-t px-5 pt-2 pb-4 md:hidden">
      {#each NAV as item (item.href)}
        <a
          class="border-ink/10 block border-b py-3 font-mono text-sm"
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
