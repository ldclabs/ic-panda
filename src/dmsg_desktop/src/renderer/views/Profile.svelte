<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
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

  const load = () =>
    attempt(async () => {
      mine = await call('profile/show')
      displayName = mine?.profile.display_name ?? ''
      bio = mine?.profile.bio ?? ''
      links = (mine?.profile.links ?? []).join('\n')
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
    })

  onMount(() => {
    if (status?.state === 'unlocked') void load()
  })
</script>

<div class="card" data-testid="profile">
  <h2>{t('publicProfile')}</h2>
  {#if error}<p class="error">{error}</p>{/if}
  {#if status?.state === 'unlocked'}
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
      <dt>{t('displayName')}</dt>
      <dd>{found.profile.display_name || '-'}</dd>
      <dt>{t('bio')}</dt>
      <dd>{found.profile.bio || '-'}</dd>
      {#each found.profile.links as link (link)}<dt>link</dt><dd class="mono">{link}</dd>{/each}
      <dt>{t('device')}</dt>
      <dd class="mono">{found.device_id}</dd>
    </dl>
  {:else if found === null}
    <p class="muted">-</p>
  {/if}
</div>
