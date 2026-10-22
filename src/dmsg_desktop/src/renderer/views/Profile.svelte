<script lang="ts">
  import { onMount } from 'svelte'
  import Avatar from '../components/Avatar.svelte'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Field from '../components/page/Field.svelte'
  import Id from '../components/page/Id.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Section from '../components/page/Section.svelte'
  import Select from '../components/Select.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, call, errorText } from '../rpc'

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
  let archives = $state<any[]>([])
  let archive = $state('')
  let fromLegacy = $state(false)
  /** Data URLs of the avatars shown, by upload ID. */
  let images = $state<Record<string, string>>({})

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try {
      await work()
    } catch (e) {
      error = errorText(e)
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

  const load = () => attempt(async () => {
    archives = (await call<any[]>('legacy/list')).sort((a, b) => b.imported_at - a.imported_at)
    archive = archives[0]?.id ?? ''
    show(await call('profile/show'))
  })

  const useLegacy = () => attempt(async () => {
    const old: any = await call('legacy/report', { id: archive, limit: 0 })
    displayName = old.profile.name
    bio = old.profile.bio
    fromLegacy = true
  })

  const avatar = (remove: boolean) =>
    attempt(async () => {
      const published = await bridge(() => window.dmsg.avatar(remove))
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
      fromLegacy = false
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

{#snippet picture(published: any, size: string)}
  {#if published?.profile.avatar_upload && images[published.profile.avatar_upload]}
    <img class="{size} shrink-0 rounded-full object-cover ring-1 ring-line" src={images[published.profile.avatar_upload]} alt={t('avatar')} data-testid={published === mine ? 'profile-avatar' : undefined} />
  {:else}
    <Avatar id={published?.account ?? status?.account ?? ''} name={published?.profile.display_name || published?.account || status?.account || ''} size="lg" class="{size} rounded-full" />
  {/if}
{/snippet}

{#if error}<Notice tone="danger">{error}</Notice>{/if}
<Section title={t('publicProfile')} description={t('avatarHint')} icon="profile" testid="profile">
  {#if status?.state === 'unlocked'}
    <div class="grid gap-4">
      {#if archives.length}
        <div class="flex flex-wrap items-center gap-2">
          <Select bind:value={archive} label={t('legacyArchives')} options={archives.map((a) => ({ value: a.id, label: a.principal, mono: true }))} testid="profile-legacy-archive" />
          <button class={buttonClass('secondary', 'sm')} disabled={busy || !archive} onclick={useLegacy} data-testid="profile-use-legacy">{t('legacyUseProfile')}</button>
        </div>
      {/if}
      {#if fromLegacy}<Notice>{t('legacyProfileDraft')}</Notice>{/if}
      <div class="flex items-center gap-4">
        {@render picture(mine, 'size-16')}
        <div class="flex flex-wrap gap-1.5">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => avatar(false)} data-testid="avatar-set"><Icon name="upload" />{t('setAvatar')}</button>
          {#if mine?.profile.avatar_upload}
            <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => avatar(true)} data-testid="avatar-remove">{t('removeAvatar')}</button>
          {/if}
        </div>
      </div>
      <Field label={t('displayName')}><input bind:value={displayName} data-testid="profile-name" /></Field>
      <Field label={t('bio')}><textarea bind:value={bio} rows="3"></textarea></Field>
      <Field label={t('links')} hint={t('linksHint')}><textarea bind:value={links} rows="3" class="font-mono text-[12px]"></textarea></Field>
      <div class="flex flex-wrap items-center gap-3">
        {#if mine}<span class="text-[12px] text-faint">v{mine.profile.version} · {t('device')} <span class="font-mono">{mine.device_id.slice(0, 12)}</span></span>{/if}
        <span class="flex-1"></span>
        <button class={buttonClass('primary')} disabled={busy} onclick={publish}>{t('publish')}</button>
      </div>
    </div>
  {/if}
</Section>

<Section title={t('resolveProfile')} icon="search">
  <div class="grid gap-3">
    <div class="flex gap-2">
      <input placeholder={t('accountId')} bind:value={other} class="font-mono text-[12.5px] placeholder:font-sans" />
      <button class={buttonClass('secondary')} disabled={busy || !other.trim()} onclick={resolve}>{t('lookUp')}</button>
    </div>
    {#if found}
      <div class="flex items-start gap-3 rounded-xl border border-line p-3.5" data-testid="profile-found">
        {@render picture(found, 'size-12')}
        <dl class="min-w-0 flex-1 divide-y divide-line">
          <Fact label={t('displayName')}>{found.profile.display_name || '-'}</Fact>
          <Fact label={t('bio')}>{found.profile.bio || '-'}</Fact>
          {#each found.profile.links as link, i (i)}<Fact label={t('link')}><bdi dir="ltr" class="font-mono text-[12px]">{link}</bdi></Fact>{/each}
          <Fact label={t('device')}><Id id={found.device_id} /></Fact>
        </dl>
        {#if status?.state === 'unlocked' && found.account !== status.account}
          <button
            class={buttonClass('secondary', 'sm', 'shrink-0')}
            onclick={() => (location.hash = `/channels?direct=${found.account}&name=${encodeURIComponent(found.profile.display_name || '')}`)}
            data-testid="profile-converse"><Icon name="channels" />{t('startConversation')}</button
          >
        {/if}
      </div>
    {:else if found === null}
      <p class="text-[12.5px] text-faint">-</p>
    {/if}
  </div>
</Section>
