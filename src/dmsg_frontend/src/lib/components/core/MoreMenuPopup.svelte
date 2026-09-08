<script lang="ts">
  import { authStore } from '$lib/stores/auth'
  import { shortId } from '$lib/utils/auth'
  import { Popover } from 'bits-ui'
  import type { Snippet } from 'svelte'

  let {
    triggerClass = '',
    trigger
  }: { triggerClass?: string; trigger: Snippet } = $props()
  let open = $state(false)
  let loading = $state(false)
  let loaded = $state(false)
  let switching = $state(false)
  let error = $state('')
  let accounts: { name: string; id: string }[] = $state([])
  const currentId = $derived($authStore.identity.getPrincipal().toText())

  async function loadAccounts() {
    if (loading) return
    loading = true
    error = ''
    try {
      const result = await authStore.nameIdentityAPI.get_my_accounts()
      accounts = result.map((account) => ({
        name: account.name,
        id: account.account.toText()
      }))
      loaded = true
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not load your username accounts.'
    } finally {
      loading = false
    }
  }

  async function switchAccount(name: string) {
    switching = true
    error = ''
    try {
      await authStore.switch(name)
    } catch (cause) {
      error =
        cause instanceof Error ? cause.message : 'Could not switch account.'
      switching = false
    }
  }
</script>

<Popover.Root
  bind:open
  onOpenChange={(next) => {
    if (next && !loaded) void loadAccounts()
  }}
>
  <Popover.Trigger class={triggerClass}>{@render trigger()}</Popover.Trigger>
  <Popover.Portal>
    <Popover.Content
      side="bottom"
      align="end"
      sideOffset={8}
      class="archive-account-menu z-[10001] w-72 max-w-[calc(100vw-32px)] rounded-xl border border-[#DAE3D9] bg-white p-3 text-sm text-[#10251F] shadow-lg"
    >
      <p class="px-3 py-2 font-semibold">Legacy account · Read-only</p>
      <a href="/" data-sveltekit-reload>Back to dMsg</a>
      <a href="/legacy">Archive information</a>
      {#if $authStore.identity.getPrincipal().isAnonymous()}
        <a href="/legacy">Sign in to read your history</a>
      {:else}
        {#if authStore.srcIdentity}
          <button
            disabled={switching ||
              currentId === authStore.srcIdentity.getPrincipal().toText()}
            onclick={() => switchAccount('')}
            >Original identity · {shortId(
              authStore.srcIdentity.getPrincipal().toText()
            )}</button
          >
        {/if}
        {#each accounts as account (account.id)}<button
            disabled={switching || currentId === account.id}
            onclick={() => switchAccount(account.name)}
            >@{account.name}{currentId === account.id
              ? ' · Current'
              : ''}</button
          >{/each}
        {#if loading}<p class="px-3 py-2" role="status"
            >Loading username accounts…</p
          >{/if}
        {#if switching}<p class="px-3 py-2" role="status">Switching identity…</p
          >{/if}
        {#if error}<p class="px-3 py-2 text-red-800" role="alert">{error}</p
          ><button onclick={loadAccounts}>Retry account list</button>{/if}
        <button onclick={() => authStore.logout('/legacy')}>Sign out</button>
      {/if}
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

<style>
  :global(.archive-account-menu) {
    font-family:
      Inter,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      sans-serif;
  }
  :global(.archive-account-menu > a),
  :global(.archive-account-menu > button) {
    display: block;
    width: 100%;
    min-height: 44px;
    padding: 12px;
    text-align: left;
    border-radius: 8px;
    overflow-wrap: anywhere;
  }
  :global(.archive-account-menu > a:hover),
  :global(.archive-account-menu > button:hover) {
    background: #edf1ea;
  }
  :global(.archive-account-menu > button:disabled) {
    color: #4e6257;
    cursor: default;
  }
  :global(.archive-account-menu :focus-visible) {
    outline: 3px solid #145c45;
    outline-offset: 2px;
  }
</style>
