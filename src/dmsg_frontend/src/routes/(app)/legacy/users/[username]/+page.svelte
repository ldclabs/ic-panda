<script lang="ts">
  import { t } from '$lib/i18n'
  import { page } from '$app/state'
  import UtilityPage from '$lib/components/site/UtilityPage.svelte'
  import { authStore } from '$lib/stores/auth'
  import { MyMessageState } from '$lib/stores/message'
  import ProfileDetail from '$src/lib/components/messages/ProfileDetail.svelte'
  import { onMount } from 'svelte'

  let myState: MyMessageState | undefined = $state()
  let error = $state('')
  let username = $derived((page?.params || {})['username'] || '')
  let pageKey = $derived($authStore.identity.getPrincipal() + ':' + username)

  async function load() {
    error = ''
    try {
      myState = await MyMessageState.load()
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not load legacy profile.'
    }
  }
  onMount(() => {
    void load()
  })
</script>

<svelte:head>
  <title>{$t('dMsg — Legacy profile')}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<UtilityPage>
  <h1>{$t('Legacy profile · Read-only')}</h1>
  <div class="legacy-profile-content">
    {#key pageKey}
      {#if myState}
        <ProfileDetail userId={username} {myState} />
      {:else if error}
        <p class="utility-status error" role="alert">{$t(error)}</p>
        <button class="button secondary utility-action" onclick={load}
          >{$t('Try again')}</button
        >
      {:else}
        <p class="utility-status" role="status"
          >{$t('Loading legacy profile…')}</p
        >
      {/if}
    {/key}
  </div>
  <a class="text-link archive-back" href="/legacy"
    >{$t('Back to archive information')}</a
  >
</UtilityPage>

<style>
  .legacy-profile-content {
    margin-top: 28px;
  }
  .archive-back {
    margin-top: 24px;
  }
</style>
