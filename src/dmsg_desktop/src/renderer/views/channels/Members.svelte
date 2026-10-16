<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Avatar from '../../components/Avatar.svelte'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { shortId } from '../../shell/session'
  import { ordered, roleLabel, roles } from './channels'

  /**
   * The members drawer: each member's role, the epoch they joined at and
   * the history granted to them. The owner changes roles and rotation and
   * removes members; the owner and admins grant history and invite.
   */
  let {
    ledger,
    me,
    owner,
    manages,
    busy,
    onControl,
    onHistory,
    onInvite,
    onClose
  }: {
    ledger: any
    me: string | undefined
    owner: boolean
    manages: boolean
    busy: boolean
    onControl: (action: Record<string, unknown>) => void
    onHistory: (member: any) => void
    onInvite: (account: string) => Promise<string | null>
    onClose: () => void
  } = $props()

  let account = $state('')
  let invitation = $state('')

  const members = $derived(ordered(ledger))

  async function invite() {
    const made = await onInvite(account.trim())
    if (made) {
      invitation = made
      account = ''
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col" data-testid="channel-members">
  <header class="flex h-12 shrink-0 items-center gap-2 border-b border-line ps-4 pe-2">
    <Icon name="members" class="text-faint" />
    <b class="flex-1 text-[13px] font-semibold">{t('members')} <span class="font-normal text-faint">{members.length}</span></b>
    <button class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('close')} onclick={onClose}><Icon name="close" /></button>
  </header>
  <ul class="min-h-0 flex-1 overflow-y-auto p-2">
    {#each members as member (member.account)}
      {@const editable = member.account !== me && member.role !== 'owner' && (owner || (manages && member.joined_epoch > 1))}
      <li class="flex items-start gap-2.5 rounded-lg px-2 py-2 hover:bg-wash/60">
        <Avatar id={member.account} />
        <div class="min-w-0 flex-1">
          <div class="flex min-w-0 items-center gap-1.5">
            <Tooltip content={member.account}>
              {#snippet children(props)}
                <button {...props} class="min-w-0 truncate font-mono text-[12px] outline-none hover:text-forest focus-visible:underline" onclick={() => copyText(member.account)}>
                  <bdi dir="ltr">{shortId(member.account)}</bdi>
                </button>
              {/snippet}
            </Tooltip>
            {#if member.account === me}<span class="rounded bg-wash px-1 text-[10.5px] text-muted">{t('you')}</span>{/if}
          </div>
          <p class="mt-0.5 text-[11.5px] text-faint">
            <span class={cn(member.role === 'owner' && 'font-medium text-forest')}>{t(roleLabel[member.role] ?? 'roleMember')}</span>
            · {t('joinedEpoch', { epoch: member.joined_epoch })}
          </p>
          {#if member.can_rotate || member.history.length}
            <div class="mt-1 flex flex-wrap gap-1">
              {#if member.can_rotate}
                <span class="inline-flex items-center gap-1 rounded bg-wash px-1.5 text-[10.5px] leading-[18px] text-ink-2"><Icon name="epoch" class="size-3" />{t('canRotate')}</span>
              {/if}
              {#if member.history.length}
                <span class="rounded bg-wash px-1.5 text-[10.5px] leading-[18px] text-ink-2">
                  {t('history')} {member.history.map((h: number[]) => `${h[0]}–${h[1]}`).join(', ')}
                </span>
              {/if}
            </div>
          {/if}
        </div>
        {#if editable}
          <DropdownMenu.Root dir={localeDir(locale())}>
            <DropdownMenu.Trigger class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('moreActions')} disabled={busy} data-testid="member-actions">
              <Icon name="more" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={4} class={cn(menuContentClass, 'min-w-52')} data-testid="member-actions-menu">
                {#if owner}
                  <DropdownMenu.RadioGroup
                    value={member.role}
                    onValueChange={(role) => role !== member.role && onControl({ type: 'role', account: member.account, role, can_rotate: member.can_rotate })}
                  >
                    <DropdownMenu.GroupHeading class="px-2.5 pt-1.5 pb-1 text-[11px] font-medium text-faint">{t('role')}</DropdownMenu.GroupHeading>
                    {#each roles as role (role)}
                      <DropdownMenu.RadioItem value={role} class={menuItemClass} data-value={role}>
                        {#snippet children({ checked })}
                          {t(roleLabel[role])}
                          {#if checked}<Icon name="check" class="size-3.5" />{/if}
                        {/snippet}
                      </DropdownMenu.RadioItem>
                    {/each}
                  </DropdownMenu.RadioGroup>
                  <DropdownMenu.CheckboxItem
                    checked={member.can_rotate}
                    onCheckedChange={(on) => onControl({ type: 'role', account: member.account, role: member.role, can_rotate: on })}
                    class={menuItemClass}
                  >
                    {#snippet children({ checked })}
                      {t('allowRotate')}
                      {#if checked}<Icon name="check" class="size-3.5" />{/if}
                    {/snippet}
                  </DropdownMenu.CheckboxItem>
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                {/if}
                {#if manages && member.joined_epoch > 1}
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => onHistory(member)} data-testid="member-history">
                    <span class="flex items-center gap-2"><Icon name="legacy" class="text-faint" />{t('grantHistory')}…</span>
                  </DropdownMenu.Item>
                {/if}
                {#if owner}
                  <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => onControl({ type: 'remove', account: member.account })}>
                    <span class="flex items-center gap-2"><Icon name="leave" />{t('removeMember')}</span>
                  </DropdownMenu.Item>
                {/if}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </li>
    {/each}
  </ul>
  {#if manages && ledger.status !== 'archived'}
    <div class="grid gap-2 border-t border-line p-3">
      <div class="flex gap-1.5">
        <input bind:value={account} placeholder={t('accountId')} class="font-mono text-[12px] placeholder:font-sans" spellcheck="false" data-testid="invite-account" />
        <button class={buttonClass('secondary')} disabled={busy || !account.trim()} onclick={invite}>{t('invite')}</button>
      </div>
      {#if invitation}
        <div class="grid gap-1.5 rounded-xl border border-line bg-surface p-2.5">
          <p class="text-[11.5px] text-muted">{t('invitationHint')}</p>
          <textarea readonly class="min-h-16 font-mono text-[11px]" data-testid="channel-invitation">{invitation}</textarea>
          <button class={buttonClass('secondary', 'sm', 'justify-self-start')} onclick={() => copyText(invitation)}><Icon name="copy" />{t('copy')}</button>
        </div>
      {/if}
    </div>
  {/if}
</div>
