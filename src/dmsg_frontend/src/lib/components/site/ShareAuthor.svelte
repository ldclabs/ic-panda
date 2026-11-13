<script lang="ts">
  import { readProfile, type ProfilePage } from '$lib/public-profile'
  import { t } from '$lib/i18n'

  let { account }: { account: string } = $props()
  let profile = $state<ProfilePage | null>(null)
  let avatarFailed = $state(false)

  $effect(() => {
    const input = account
    const controller = new AbortController()
    profile = null
    avatarFailed = false
    // Independent of the file's metadata and download state; the link is usable now.
    void readProfile(input, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted && value.account === input)
          profile = value
      })
      .catch(() => {})
    return () => controller.abort()
  })
</script>

<aside
  class="share-author"
  aria-label={$t('Shared by')}
  data-testid="share-author"
>
  {#if profile?.document?.avatar_url && !avatarFailed}
    <img
      src={profile.document.avatar_url}
      alt={$t('Public profile avatar')}
      referrerpolicy="no-referrer"
      onerror={() => (avatarFailed = true)}
    />
  {/if}
  <div>
    <span class="label">{$t('Shared by')}</span>
    <a
      class="text-link"
      href={`/${account}`}
      target="_blank"
      rel="noopener noreferrer"
      referrerpolicy="no-referrer"
      data-testid="share-profile"
    >
      <bdi
        >{profile?.document?.name ||
          (profile?.handle
            ? `@${profile.handle}`
            : $t('Check their public profile'))}</bdi
      >
      {#if profile?.handle && profile.document?.name}<span
          class="handle"
          dir="ltr">@{profile.handle}</span
        >{/if}
    </a>
  </div>
</aside>

<style>
  .share-author {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 24px;
    font-size: 14px;
  }
  .share-author img {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    object-fit: cover;
    flex: none;
  }
  .share-author div {
    min-width: 0;
  }
  .label {
    display: block;
    color: var(--muted);
    font-size: 12px;
  }
  .share-author a {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    overflow-wrap: anywhere;
  }
  .handle {
    color: var(--muted);
    font-size: 12px;
  }
</style>
