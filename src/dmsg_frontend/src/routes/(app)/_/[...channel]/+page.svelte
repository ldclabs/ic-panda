<script lang="ts">
  import { t } from '$lib/i18n'
  import { goto } from '$app/navigation'
  import Chat from '$lib/components/messages/Chat.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { onMount } from 'svelte'

  let myState: MyMessageState | null = $state(null)
  let error = $state('')
  async function load() {
    error = ''
    try {
      const state = await MyMessageState.load()
      if (state.principal.isAnonymous() || !state.api.myInfo)
        return goto('/legacy')
      myState = state
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not load legacy messages.'
    }
  }
  onMount(() => {
    void load()
  })
</script>

<svelte:head
  ><title>{$t('dMsg — Legacy messages')}</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
{#if myState}<Chat {myState} />
{:else if error}<div class="archive-empty"
    ><h1>{$t('Could not load your archive')}</h1><p role="alert">{$t(error)}</p
    ><button class="button secondary" onclick={load}>{$t('Try again')}</button
    ><a class="text-link" href="/legacy">{$t('Back to archive information')}</a
    ></div
  >
{:else}<p class="archive-empty" role="status"
    >{$t('Loading legacy messages…')}</p
  >{/if}
