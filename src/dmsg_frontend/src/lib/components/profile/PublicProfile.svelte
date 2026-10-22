<script lang="ts">
  import Brand from '$lib/components/landing/Brand.svelte'
  import '$lib/components/landing/site.css'
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import { APP_ORIGIN } from '$lib/constants'
  import {
    readProfile,
    ProfileError,
    type ProfilePage
  } from '$lib/public-profile'

  let { identifier }: { identifier: string } = $props()
  let result = $state<ProfilePage | null>(null)
  let error = $state<ProfileError['reason'] | null>(null)
  let busy = $state(true)
  let retry = $state(0)
  let avatarFailed = $state(false)
  let copied = $state(false)

  $effect(() => {
    const input = identifier
    void retry
    const controller = new AbortController()
    busy = true
    result = null
    error = null
    avatarFailed = false
    copied = false
    readProfile(input, controller.signal)
      .then((value) => {
        if (!controller.signal.aborted) result = value
      })
      .catch((cause) => {
        if (!controller.signal.aborted)
          error = cause instanceof ProfileError ? cause.reason : 'unavailable'
      })
      .finally(() => {
        if (!controller.signal.aborted) busy = false
      })
    return () => controller.abort()
  })

  const profile = $derived(result?.document)
  const title = $derived(
    profile?.name ||
      (result?.handle ? `@${result.handle}` : $t('Public profile'))
  )
  const linkImages = $derived(
    (profile?.extra?.['dmsg.link_images'] ?? {}) as Record<string, string>
  )
  const permalink = $derived(result ? `${APP_ORIGIN}/${result.account}` : '')
  const message = $derived(
    error
      ? {
          invalid_target: $t('This is not a valid account ID or handle.'),
          not_found: $t('This account or handle was not found.'),
          legacy_reserved: $t(
            'This legacy handle is reserved and has not been linked to a new account.'
          ),
          unavailable: $t('Could not load this profile. Try again.'),
          verification_failed: $t(
            'This profile could not be verified. Its content is not displayed.'
          )
        }[error]
      : ''
  )

  async function copyLink() {
    try {
      await navigator.clipboard.writeText(permalink)
      copied = true
    } catch {
      copied = false
    }
  }
</script>

<svelte:head>
  <title>{`${title} — dMsg`}</title>
  <meta name="color-scheme" content="light" />
  {#if permalink}<link rel="canonical" href={permalink} />{/if}
</svelte:head>

<div class="dmsg-site profile-site">
  <a class="skip" href="#profile-main">{$t('Skip to content')}</a>
  <header class="site-header"
    ><div class="wrap nav-wrap"><Brand /><LocaleSwitcher /></div></header
  >
  <main id="profile-main" tabindex="-1" aria-busy={busy}>
    <div class="profile-wrap">
      <p class="eyebrow">{$t('Public profile')}</p>
      {#if busy}
        <section class="status" role="status"
          ><div class="placeholder" aria-hidden="true"></div><h1
            >{$t('Loading and verifying profile…')}</h1
          ></section
        >
      {:else if error}
        <section class="status" role="alert"
          ><h1
            >{error === 'legacy_reserved'
              ? $t('Legacy profile · Read-only')
              : $t('Profile unavailable')}</h1
          ><p>{message}</p>
          {#if error === 'legacy_reserved'}
            <a
              class="button primary"
              href="/legacy/users/{encodeURIComponent(
                identifier.toLowerCase()
              )}">{$t('View legacy profile')}</a
            >
          {:else}<button class="button primary" onclick={() => retry++}
              >{$t('Try again')}</button
            >{/if}</section
        >
      {:else if result}
        <section class="identity" aria-labelledby="profile-name">
          {#if profile?.avatar_url && !avatarFailed}
            <img
              class="avatar"
              src={profile?.avatar_url}
              alt={$t('Public profile avatar')}
              referrerpolicy="no-referrer"
              onerror={() => (avatarFailed = true)}
            />
          {:else}<div class="avatar monogram" aria-hidden="true"
              >{[
                ...(profile?.name || result.handle || result.account)
              ][0]?.toUpperCase()}</div
            >{/if}
          <div class="identity-text"
            ><h1 id="profile-name">{title}</h1>
            {#if result.handle}<p class="handle">@{result.handle}</p>{/if}
            <p class="account-label">{$t('Account ID')}</p><a
              class="account"
              href="/{result.account}"
              dir="ltr">{result.account}</a
            >
          </div>
        </section>
        {#if profile}
          {#if profile.description}<p class="bio">{profile.description}</p>{/if}
          {#if profile.links?.length}<ul
              class="profile-links"
              aria-label={$t('Links')}
            >
              {#each profile.links ?? [] as link}<li
                  ><a
                    href={link.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    referrerpolicy="no-referrer"
                    >{#if linkImages[link.url]}<img
                        class="link-image"
                        src={linkImages[link.url]}
                        alt=""
                        referrerpolicy="no-referrer"
                      />{/if}{link.name}<span aria-hidden="true">↗</span></a
                  ></li
                >{/each}
            </ul>{/if}
          <section class="verification" aria-labelledby="verification-title">
            <h2 id="verification-title"
              ><span aria-hidden="true">✓</span>
              {$t('Certified principal document')}</h2
            >
            <p
              >{$t(
                'These details come from the same certified identity document read by agents. The account publishes its own claims.'
              )}</p
            >
            <dl>
              <div
                ><dt>{$t('Principal ID')}</dt><dd
                  ><a
                    href={profile.id}
                    target="_blank"
                    rel="noopener noreferrer">{profile.id}</a
                  ></dd
                ></div
              >
              <div
                ><dt>{$t('Updated')}</dt><dd
                  >{new Date(profile.updated_at).toLocaleString()}</dd
                ></div
              >
            </dl>
          </section>
        {:else}<p class="empty" role="status"
            >{$t('This account has not published a public profile.')}</p
          >{/if}
        <div class="profile-actions"
          ><button class="button secondary" onclick={copyLink}
            >{copied ? $t('Link copied') : $t('Copy permanent link')}</button
          ><a class="button primary" href="/">{$t('Get dMsg')}</a></div
        >
        <p class="permalink"><a href={permalink} dir="ltr">{permalink}</a></p>
      {/if}
    </div>
  </main>
  <footer class="profile-footer"
    ><a href="/">{$t('Back to dMsg')}</a><a href="/_/verify"
      >{$t('Verify a dMsg signature')}</a
    ></footer
  >
</div>

<style>
  .profile-wrap {
    max-width: 760px;
    margin: auto;
    padding: 64px 28px 48px;
  }
  .eyebrow {
    font-size: 12px;
    letter-spacing: 0.15em;
    text-transform: uppercase;
  }
  .identity {
    display: flex;
    gap: 28px;
    align-items: center;
    margin: 36px 0;
  }
  .identity-text {
    min-width: 0;
  }
  h1 {
    font-size: clamp(28px, 5vw, 44px);
    overflow-wrap: anywhere;
  }
  .avatar {
    width: 112px;
    height: 112px;
    border-radius: 50%;
    object-fit: cover;
    flex: 0 0 112px;
    background: var(--pale);
  }
  .monogram {
    display: grid;
    place-items: center;
    color: var(--action);
    font-size: 44px;
  }
  .identity .handle {
    margin: 8px 0;
    color: var(--action);
  }
  .identity .account-label {
    font-size: 11px;
    margin: 12px 0 0;
  }
  .account {
    font: 14px/1.6 monospace;
    overflow-wrap: anywhere;
  }
  .bio {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-size: 18px;
  }
  .profile-links {
    list-style: none;
    padding: 0;
    margin: 28px 0;
  }
  .link-image {
    width: 40px;
    height: 40px;
    border-radius: 8px;
    object-fit: cover;
  }
  .profile-links li {
    border-bottom: 1px solid var(--line);
  }
  .profile-links a {
    display: flex;
    gap: 20px;
    justify-content: space-between;
    padding: 14px 0;
    overflow-wrap: anywhere;
  }
  .verification {
    border-top: 1px solid var(--line);
    padding-top: 24px;
    margin: 36px 0 28px;
  }
  .verification h2 {
    font-size: 17px;
    letter-spacing: -0.02em;
    color: var(--action);
  }
  .verification p {
    font-size: 13px;
    margin: 10px 0 18px;
  }
  dl {
    font-size: 12px;
    color: var(--muted);
  }
  dl div {
    display: grid;
    grid-template-columns: 175px minmax(0, 1fr);
    gap: 12px;
    margin: 8px 0;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .profile-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .permalink {
    font-size: 12px;
    margin-top: 14px;
    overflow-wrap: anywhere;
  }
  .status {
    min-height: 320px;
    padding-top: 28px;
  }
  .status h1 {
    margin-bottom: 24px;
  }
  .placeholder {
    width: 112px;
    height: 112px;
    border-radius: 50%;
    background: var(--pale);
    margin-bottom: 28px;
  }
  .empty {
    padding-block: 28px;
    border-block: 1px solid var(--line);
  }
  .profile-footer {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 28px;
    padding: 28px;
    font-size: 12px;
    border-top: 1px solid var(--line);
  }
  @media (max-width: 540px) {
    .profile-wrap {
      padding-top: 36px;
    }
    .identity {
      align-items: flex-start;
      flex-direction: column;
      gap: 18px;
    }
    dl div {
      grid-template-columns: minmax(0, 1fr);
      gap: 2px;
    }
  }
</style>
