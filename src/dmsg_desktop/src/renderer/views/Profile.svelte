<script lang="ts">
  import { onMount } from 'svelte'
  import type { PrincipalDocument, PrincipalLink } from 'agent-protocols/delegation'
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
  import { bridge, call, code, errorText } from '../rpc'

  /** The same certified PrincipalDocument used by JSON and human readers. */
  let { status }: { status: any } = $props()

  let mine = $state<PrincipalDocument | null>(null)
  let avatarUrl = $state<string | null>(null)
  let extra = $state<Record<string, unknown>>({})
  let linkImages = $state<Record<string, string>>({})
  let pending = $state(false)
  let displayName = $state('')
  let kind = $state('person')
  let bio = $state('')
  let links = $state<PrincipalLink[]>([])
  let other = $state('')
  let found = $state<any>(undefined)
  let error = $state('')
  let busy = $state(false)
  let archives = $state<any[]>([])
  let archive = $state('')
  let fromLegacy = $state(false)
  let legacyAvatarReference = $state(false)
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

  /** Public images are loaded through the daemon, from the configured relay. */
  async function avatarOf(document: PrincipalDocument | null) {
    const url = document?.avatar_url
    if (!url || images[url]) return
    const account = document!.id.split('/').pop()!
    const upload = url.split('/').pop()!
    if (/^[0-9a-f]{64}$/.test(upload)) {
      const image: any = await call('profile/avatar_image', { account, upload }).catch(() => null)
      if (image) images = { ...images, [url]: image.url }
    }
  }

  function show(document: PrincipalDocument) {
    mine = document
    legacyAvatarReference = false
    kind = document.type ?? 'person'
    displayName = document.name ?? ''
    bio = document.description ?? ''
    avatarUrl = document.avatar_url ?? null
    links = (document.links ?? []).map(link => ({ ...link }))
    extra = structuredClone(document.extra ?? {})
    linkImages = { ...(extra['dmsg.link_images'] as Record<string, string> ?? {}) }
    void avatarOf(document)
  }

  const load = () => attempt(async () => {
    archives = (await call<any[]>('legacy/list')).sort((a, b) => b.imported_at - a.imported_at)
    archive = archives[0]?.id ?? ''
    show(await call('profile/show'))
    pending = await call<boolean>('profile/pending')
    const publication: any = await call('agent/principal')
    pending ||= !!publication && !publication.published
  })

  const useLegacy = () => attempt(async () => {
    const old: any = await call('legacy/report', { id: archive, limit: 0 })
    displayName = old.profile.name
    bio = old.profile.bio
    links = old.profile.links ?? []
    extra = old.profile.extra ?? {}
    linkImages = { ...(extra['dmsg.link_images'] as Record<string, string> ?? {}) }
    fromLegacy = true
    legacyAvatarReference = false
    if (old.profile.image) { avatarUrl = old.profile.image; legacyAvatarReference = true }
    const image = old.files.find((file: any) => file.kind === 'avatar' && file.verified)
    if (image?.mime) {
      avatarUrl = await call('profile/avatar_legacy', { archive, source: image.source })
      legacyAvatarReference = false
      await avatarOf({ ...mine!, avatar_url: avatarUrl! })
    } else if (old.profile.image) {
      // Preserve the recorded public URL when the binary could not be collected.
      avatarUrl = old.profile.image
    }
  })

  const avatar = (remove: boolean) => attempt(async () => {
    if (remove) { avatarUrl = null; legacyAvatarReference = false; return }
    const url = await bridge(() => window.dmsg.avatar())
    if (typeof url === 'string') {
      avatarUrl = url
      legacyAvatarReference = false
      await avatarOf({ ...mine!, avatar_url: url })
    }
  })

  const publish = () => attempt(async () => {
    if (!mine) return
    if (pending) {
      let outcome: any
      try { outcome = await call('profile/retry') } catch (cause) {
        if (code(cause) === 'EXPIRED') pending = false
        throw cause
      }
      if (!outcome.published) return
    } else {
      const keptExtra = { ...extra }
      const artwork = Object.fromEntries(Object.entries(linkImages).filter(([url, image]) => image && links.some(link => link.url === url)))
      if (Object.keys(artwork).length) keptExtra['dmsg.link_images'] = artwork
      else delete keptExtra['dmsg.link_images']
      let outcome: any
      try { outcome = await call('profile/publish', {
        ...mine, type: kind, name: displayName.trim() || undefined, description: bio.trim() || undefined,
        avatar_url: avatarUrl ?? undefined, links, extra: keptExtra
      }) } catch (cause) {
        if (code(cause) === 'EXECUTION_UNKNOWN') pending = true
        if (code(cause) === 'PENDING') pending = await call<boolean>('profile/pending')
        throw cause
      }
      if (!outcome.published) { pending = true; return }
    }
    pending = false
    fromLegacy = false
    show(await call('profile/show'))
  })

  function changeLinkUrl(index: number, url: string) {
    const old = links[index]!.url
    const image = linkImages[old]
    links[index]!.url = url
    if (image) linkImages[url] = image
    if (!links.some(link => link.url === old)) delete linkImages[old]
  }

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
  {#if published?.avatar_url && images[published.avatar_url]}
    <img class="{size} shrink-0 rounded-full object-cover ring-1 ring-line" src={images[published.avatar_url]} alt={t('avatar')} data-testid={'profile-avatar'} />
  {:else}
    <Avatar id={published?.id ?? status?.account ?? ''} name={published?.name || published?.id || status?.account || ''} size="lg" class="{size} rounded-full" />
  {/if}
{/snippet}

{#if error}<Notice tone="danger">{error}</Notice>{/if}
<Section title={t('publicProfile')} description={t('avatarHint')} icon="profile" testid="profile">
  {#if status?.state === 'unlocked'}
    <div class="grid gap-4">
      {#if archives.length}
        <div class="flex flex-wrap items-center gap-2">
          <Select bind:value={archive} label={t('legacyArchives')} options={archives.map((a) => ({ value: a.id, label: a.principal, mono: true }))} testid="profile-legacy-archive" />
          <button class={buttonClass('secondary', 'sm')} disabled={busy || pending || !archive} onclick={useLegacy} data-testid="profile-use-legacy">{t('legacyUseProfile')}</button>
        </div>
      {/if}
      {#if fromLegacy}<Notice>{t('legacyProfileDraft')}</Notice>{/if}
      {#if legacyAvatarReference}<Notice>{t('legacyAvatarReference')}</Notice>{/if}
      {#if pending}<Notice tone="warn">{t('principalPublicationPending')}</Notice>{/if}
      <fieldset disabled={busy || pending} class="contents">
      <div class="flex items-center gap-4">
        {@render picture(mine ? { ...mine, avatar_url: avatarUrl, name: displayName } : null, 'size-16')}
        <div class="flex flex-wrap gap-1.5">
          <button class={buttonClass('secondary', 'sm')} disabled={busy} onclick={() => avatar(false)} data-testid="avatar-set"><Icon name="upload" />{t('setAvatar')}</button>
          {#if avatarUrl}
            <button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={() => avatar(true)} data-testid="avatar-remove">{t('removeAvatar')}</button>
          {/if}
        </div>
      </div>
      {#if avatarUrl && !images[avatarUrl]}<Field label={t('avatar')}><input value={avatarUrl} readonly class="font-mono text-[12px]" /></Field>{/if}
      <Select bind:value={kind} label={t('principalKind')} options={[
        {value:'person',label:t('principalPerson')}, {value:'organization',label:t('principalOrganization')},
        {value:'team',label:t('principalTeam')}, {value:'project',label:t('principalProject')}, {value:'other',label:t('principalOther')}
      ]} testid="profile-kind" />
      <Field label={t('displayName')}><input bind:value={displayName} data-testid="profile-name" /></Field>
      <Field label={t('bio')}><textarea bind:value={bio} rows="3"></textarea></Field>
      <div class="grid gap-2">
        {#each links as link, index (index)}
          <div class="grid gap-2 rounded-xl border border-line p-3">
            <Field label={t('displayName')}><input bind:value={link.name} /></Field>
            <Field label={t('link')}><input value={link.url} oninput={(event) => changeLinkUrl(index, event.currentTarget.value)} type="url" /></Field>
            <Field label={t('principalLinkImage')}><input value={linkImages[link.url] ?? ''} oninput={(event) => linkImages[link.url] = event.currentTarget.value} type="url" /></Field>
            <Field label={t('principalLinkRelation')}><input bind:value={link.rel} /></Field>
            <button class={buttonClass('ghost', 'sm')} onclick={() => links = links.filter((_, i) => i !== index)}>{t('remove')}</button>
          </div>
        {/each}
        <button class={buttonClass('secondary', 'sm')} onclick={() => links = [...links, { name: '', url: '', rel: 'related' }]}>{t('principalAddLink')}</button>
      </div>
      </fieldset>
      <div class="flex flex-wrap items-center gap-3">
        {#if mine?.updated_at}<span class="text-[12px] text-faint">{new Date(mine.updated_at).toLocaleString()}</span>{/if}
        <span class="flex-1"></span>
        <button class={buttonClass('primary')} disabled={busy} onclick={publish}>{pending ? t('retry') : t('publish')}</button>
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
          <Fact label={t('displayName')}>{found.name || '-'}</Fact>
          <Fact label={t('bio')}>{found.description || '-'}</Fact>
          {#each (found.links ?? []) as link, i (i)}<Fact label={t('link')}><bdi dir="ltr" class="font-mono text-[12px]">{link.name} · {link.url}</bdi></Fact>{/each}
          <Fact label={t('device')}><Id id={found.id} /></Fact>
        </dl>
        {#if status?.state === 'unlocked' && found.id.split('/').pop() !== status.account}
          <button
            class={buttonClass('secondary', 'sm', 'shrink-0')}
            onclick={() => (location.hash = `/channels?direct=${found.id.split('/').pop()}&name=${encodeURIComponent(found.name || '')}`)}
            data-testid="profile-converse"><Icon name="channels" />{t('startConversation')}</button
          >
        {/if}
      </div>
    {:else if found === null}
      <p class="text-[12.5px] text-faint">-</p>
    {/if}
  </div>
</Section>
