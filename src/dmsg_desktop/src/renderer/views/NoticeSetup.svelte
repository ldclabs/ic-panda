<script lang="ts">
  import { onMount } from 'svelte'
  import Notice from '../components/page/Notice.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { call, onChange, single } from '../rpc'
  import { dismissSetup, setupDismissed } from '../notice-setup.svelte'
  import { securityAlertsOn, type NoticePreferences } from '../../shared/notifications'

  /**
   * New installations start with notifications off. Once the account is set
   * up, this points to the settings; it never asks the system for permission.
   */
  let { status, go, class: className }: { status: any; go: (path: string) => void; class?: string } = $props()
  let preferences = $state<NoticePreferences | null>(null)
  let alive = true
  const account = $derived(status?.state === 'unlocked' ? (status.account as string | undefined) ?? '' : '')
  const shown = $derived(!!account && preferences?.account_id === account && !securityAlertsOn(preferences) && !setupDismissed(account))

  const load = single(async () => {
    const id = account
    if (!id) { preferences = null; return }
    try {
      const next = await call<NoticePreferences>('notifications/settings', { account: id })
      if (alive && id === account) preferences = next
    } catch { /* No prompt without a confirmed setting. */ }
  })

  $effect(() => { void account; void load() })
  onMount(() => {
    const off = onChange(['notifications/changed'], load)
    return () => { alive = false; off() }
  })
</script>

{#if shown}
  <Notice icon="security" class={className} testid="notice-setup">
    <p class="font-medium text-ink-2">{t('noticeSetupTitle')}</p>
    <p class="mt-0.5">{t('noticeSetupBody')}</p>
    <div class="mt-2 flex flex-wrap gap-2">
      <button class={buttonClass('secondary', 'xs')} onclick={() => go('/settings/notifications')} data-testid="notice-setup-open">{t('noticeSetupOpen')}</button>
      <button class={buttonClass('ghost', 'xs')} onclick={() => dismissSetup(account)} data-testid="notice-setup-later">{t('noticeSetupLater')}</button>
    </div>
  </Notice>
{/if}
