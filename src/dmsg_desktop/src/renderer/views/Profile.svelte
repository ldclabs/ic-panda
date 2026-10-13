<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n/index.svelte'
  import { call, code } from '../rpc'

  /** This account's signed public profile, and other accounts' checked against their devices. */
  let { status }: { status: any } = $props()
  let mine = $state<any>(null)
  let displayName = $state('')
  let bio = $state('')
  let links = $state('')
  let other = $state('')
  let found = $state<any>(undefined)
  let error = $state('')
  let busy = $state(false)
  /** Data URLs of the avatars shown, by upload ID. */
  let images = $state<Record<string, string>>({})

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = code(e) === 'LOCKED' ? t('locked') : `${t('error')}: ${code(e)}`
    } finally {
      busy = false
    }
  }

  /** Fetch a profile's avatar from the relay's public URL, through the daemon. */
  async function avatarOf(published: any) {
    const upload = published?.profile.avatar_upload
    if (!upload || images[upload]) return
    const image: any = await call('profile/avatar_image', { account: published.account, upload }).catch(() => null)
    if (image) images = { ...images, [upload]: image.url }
  }

  function show(published: any) {
    mine = published
    displayName = mine?.profile.display_name ?? ''
    bio = mine?.profile.bio ?? ''
    links = (mine?.profile.links ?? []).join('\n')
    void avatarOf(mine)
  }

  const load = () => attempt(async () => show(await call('profile/show')))

  const avatar = (remove: boolean) =>
    attempt(async () => {
      const published = await window.dmsg.avatar(remove)
      if (published) show(published)
    })

  const publish = () =>
    attempt(async () => {
      mine = await call('profile/publish', {
        display_name: displayName.trim(),
        bio: bio.trim(),
        links: links
          .split('\n')
          .map((l) => l.trim())
          .filter(Boolean)
      })
    })

  const resolve = () =>
    attempt(async () => {
      found = await call('profile/resolve', { account: other.trim() })
      void avatarOf(found)
    })

  onMount(() => {
    if (status?.state === 'unlocked') void load()
  })
</script>

<div class="card" data-testid="profile">
  <h2>{t('publicProfile')}</h2>
  {#if error}<p class="error">{error}</p>{/if}
  {#if status?.state === 'unlocked'}
    <div class="row" style="margin-bottom: 8px">
      {#if mine?.profile.avatar_upload && images[mine.profile.avatar_upload]}
        <img class="avatar" src={images[mine.profile.avatar_upload]} alt={t('avatar')} data-testid="profile-avatar" />
      {/if}
      <button disabled={busy} onclick={() => avatar(false)} data-testid="avatar-set">{t('setAvatar')}</button>
      {#if mine?.profile.avatar_upload}
        <button disabled={busy} onclick={() => avatar(true)} data-testid="avatar-remove">{t('removeAvatar')}</button>
      {/if}
    </div>
    <p class="muted">{t('avatarHint')}</p>
    <input placeholder={t('displayName')} bind:value={displayName} data-testid="profile-name" />
    <textarea placeholder={t('bio')} bind:value={bio} style="margin-top: 8px"></textarea>
    <textarea placeholder={t('links')} bind:value={links} style="margin-top: 8px"></textarea>
    <div class="row" style="margin-top: 8px">
      <button class="primary" disabled={busy} onclick={publish}>{t('publish')}</button>
      {#if mine}<span class="muted">v{mine.profile.version} · {t('device')} <span class="mono">{mine.device_id.slice(0, 12)}</span></span>{/if}
    </div>
  {/if}
  <h3>{t('resolveProfile')}</h3>
  <div class="row">
    <input placeholder={t('accountId')} bind:value={other} />
    <button disabled={busy || !other.trim()} onclick={resolve}>{t('lookUp')}</button>
  </div>
  {#if found}
    <dl data-testid="profile-found">
      {#if found.profile.avatar_upload && images[found.profile.avatar_upload]}
        <dt>{t('avatar')}</dt>
        <dd><img class="avatar" src={images[found.profile.avatar_upload]} alt={t('avatar')} /></dd>
      {/if}
      <dt>{t('displayName')}</dt>
      <dd>{found.profile.display_name || '-'}</dd>
      <dt>{t('bio')}</dt>
      <dd>{found.profile.bio || '-'}</dd>
      {#each found.profile.links as link (link)}<dt>{t('link')}</dt><dd class="mono">{link}</dd>{/each}
      <dt>{t('device')}</dt>
      <dd class="mono">{found.device_id}</dd>
    </dl>
  {:else if found === null}
    <p class="muted">-</p>
  {/if}
</div>

<style>
  .avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    object-fit: cover;
  }
</style>
