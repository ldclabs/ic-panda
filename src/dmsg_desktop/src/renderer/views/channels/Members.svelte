<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { localeDir } from '../../../shared/locales'
  import Avatar from '../../components/Avatar.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Segmented from '../../components/Segmented.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { when } from '../../rpc'
  import { shortId } from '../../shell/session'
  import { invitationLink, mayRotate, ordered, roleLabel, roles } from './channels'
  import { label } from './names.svelte'

  /**
   * The members drawer: each member's name, role, the epoch they joined at
   * and the history granted to them. The owner changes roles and rotation;
   * the owner removes anyone else and admins remove members and publishers,
   * once confirmed. Managers invite by account ID or name, as a member or a
   * publisher; an invitation made while the epoch is active carries its key,
   * so the member reads it from its start, and rotating first keeps what was
   * written from them. Open invitations list with a link to hand over again
   * and a withdraw.
   */
  let {
    channel,
    ledger,
    me,
    owner,
    manages,
    rotates,
    busy,
    onControl,
    onHistory,
    onInvite,
    onClose
  }: {
    channel: any
    ledger: any
    me: string | undefined
    owner: boolean
    manages: boolean
    rotates: boolean
    busy: boolean
    onControl: (action: Record<string, unknown>) => void
    onHistory: (member: any) => void
    onInvite: (target: string, role: string, rekey: boolean) => Promise<any | null>
    onClose: () => void
  } = $props()

  let target = $state('')
  let role = $state('member')
  let rekey = $state(false)
  let invitation = $state<any>(null)
  let removing = $state<any>(null)

  const members = $derived(ordered(ledger))
  /** Open invitations: those this device made carry their packet. */
  const open = $derived(
    Object.entries(ledger.invitations ?? {})
      .filter(([, i]: [string, any]) => i.expires_at > Date.now() && i.epoch === ledger.epoch)
      .map(([id, i]: [string, any]) => ({ id, ...i, packet: channel.invites?.[id] ?? null }))
  )

  async function invite() {
    const made = await onInvite(target.trim(), role, rekey && ledger.status === 'active')
    if (made) {
      invitation = made
      target = ''
      rekey = false
    }
  }

  /** Who may remove a member: the owner anyone else, an admin members and publishers. */
  const removable = (member: any) => member.account !== me && member.role !== 'owner' && (owner || (manages && (member.role === 'member' || member.role === 'publisher')))
</script>

{#snippet handOver(packet: any)}
  <div class="grid gap-1.5 rounded-xl border border-line bg-surface p-2.5" data-testid="channel-invitation-box">
    <p class="text-[11.5px] text-muted">{t('invitationHint')}</p>
    <input readonly value={invitationLink(packet)} class="font-mono text-[11px]" data-testid="channel-invitation-link" />
    <textarea readonly class="hidden" data-testid="channel-invitation">{JSON.stringify(packet)}</textarea>
    <div class="flex gap-1.5">
      <button class={buttonClass('secondary', 'sm')} onclick={() => copyText(invitationLink(packet))} data-testid="invitation-copy-link"><Icon name="link" />{t('copyLink')}</button>
      <button class={buttonClass('ghost', 'sm')} onclick={() => copyText(JSON.stringify(packet))}><Icon name="copy" />{t('copyJson')}</button>
    </div>
  </div>
{/snippet}

<div class="flex h-full min-h-0 flex-col" data-testid="channel-members">
  <header class="flex h-12 shrink-0 items-center gap-2 border-b border-line ps-4 pe-2">
    <Icon name="members" class="text-faint" />
    <b class="flex-1 text-[13px] font-semibold">{t('members')} <span class="font-normal text-faint">{members.length}</span></b>
    <button class={iconButtonClass('ghost', 'lg')} aria-label={t('close')} onclick={onClose}><Icon name="close" /></button>
  </header>
  <ul class="min-h-0 flex-1 overflow-y-auto p-2">
    {#each members as member (member.account)}
      {@const editable = member.account !== me && member.role !== 'owner' && (owner || removable(member) || (manages && member.joined_epoch > 1))}
      <li class="flex items-start gap-2.5 rounded-lg px-2 py-2 hover:bg-wash/60">
        <Avatar id={member.account} name={label(member.account)} />
        <div class="min-w-0 flex-1">
          <div class="flex min-w-0 items-center gap-1.5">
            <Tooltip content={member.account}>
              {#snippet children(props)}
                <button {...props} class="min-w-0 truncate text-[13px] font-medium outline-none hover:text-forest focus-visible:underline" onclick={() => copyText(member.account)}>
                  <bdi>{label(member.account)}</bdi>
                </button>
              {/snippet}
            </Tooltip>
            {#if member.account === me}<span class="rounded bg-wash px-1 text-[10.5px] text-muted">{t('you')}</span>{/if}
          </div>
          <p class="mt-0.5 text-[11.5px] text-faint">
            <span class={cn(member.role === 'owner' && 'font-medium text-forest')}>{t(roleLabel[member.role] ?? 'roleMember')}</span>
            · {t('joinedEpoch', { epoch: member.joined_epoch })}
          </p>
          {#if mayRotate(ledger, member.account) || member.history.length}
            <div class="mt-1 flex flex-wrap gap-1">
              {#if mayRotate(ledger, member.account)}
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
            <DropdownMenu.Trigger class={iconButtonClass('ghost', 'md')} aria-label={t('moreActions')} disabled={busy} data-testid="member-actions">
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
                  {#if member.role !== 'admin'}
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
                  {/if}
                  <DropdownMenu.Separator class="my-1 h-px bg-line" />
                {/if}
                {#if manages && member.joined_epoch > 1}
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => onHistory(member)} data-testid="member-history">
                    <span class="flex items-center gap-2"><Icon name="legacy" class="text-faint" />{t('grantHistory')}…</span>
                  </DropdownMenu.Item>
                {/if}
                {#if removable(member)}
                  <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (removing = member)} data-testid="member-remove">
                    <span class="flex items-center gap-2"><Icon name="leave" />{t('removeMember')}…</span>
                  </DropdownMenu.Item>
                {/if}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </li>
    {/each}
    {#if open.length}
      <li class="px-2 pt-3 pb-1 text-[11px] font-medium text-faint">{t('openInvitations')}</li>
      {#each open as item (item.id)}
        <li class="flex items-center gap-2.5 rounded-lg px-2 py-1.5" data-testid="open-invitation">
          <Avatar id={item.account} name={label(item.account)} size="sm" />
          <div class="min-w-0 flex-1">
            <p class="truncate text-[12.5px]"><bdi>{label(item.account)}</bdi></p>
            <p class="text-[11px] text-faint">{t(roleLabel[item.role] ?? 'roleMember')} · {t('expires')} {when(item.expires_at)}</p>
          </div>
          {#if item.packet}
            <Tooltip content={t('copyLink')}>
              {#snippet children(props)}
                <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('copyLink')} onclick={() => copyText(invitationLink(item.packet))}><Icon name="link" /></button>
              {/snippet}
            </Tooltip>
          {/if}
          {#if manages}
            <Tooltip content={t('withdrawInvitation')}>
              {#snippet children(props)}
                <button
                  {...props}
                  class={iconButtonClass('ghost', 'md')}
                  aria-label={t('withdrawInvitation')}
                  disabled={busy}
                  onclick={() => onControl({ type: 'revoke_invite', invitation_id: item.id })}
                  data-testid="invitation-revoke"><Icon name="close" /></button
                >
              {/snippet}
            </Tooltip>
          {/if}
        </li>
      {/each}
    {/if}
  </ul>
  {#if manages && ledger.status !== 'archived'}
    <div class="grid gap-2 border-t border-line p-3">
      <div class="flex gap-1.5">
        <input bind:value={target} placeholder={t('accountOrName')} class="min-w-0 font-mono text-[12px] placeholder:font-sans" spellcheck="false" data-testid="invite-account" />
        <button class={buttonClass('secondary')} disabled={busy || !target.trim()} onclick={invite} data-testid="invite-submit">{t('invite')}</button>
      </div>
      <Segmented
        bind:value={role}
        options={[
          { value: 'member', label: t('roleMember') },
          { value: 'publisher', label: t('rolePublisher') }
        ]}
        label={t('role')}
        testid="invite-role"
      />
      {#if ledger.status === 'active'}
        <p class="text-[11.5px] text-faint" data-testid="invite-reads">{t('inviteReads', { epoch: rekey ? ledger.epoch + 1 : ledger.epoch })}</p>
        {#if rotates}<Checkbox bind:checked={rekey} class="text-[12px]" testid="invite-rekey">{t('inviteRekey')}</Checkbox>{/if}
      {:else}
        <p class="text-[11.5px] text-faint" data-testid="invite-reads">{t('inviteNext')}</p>
      {/if}
      {#if invitation}{@render handOver(invitation)}{/if}
    </div>
  {/if}
</div>

{#if removing}
  <Dialog
    bind:open={() => !!removing, (open) => !open && (removing = null)}
    title={t('removeTitle', { name: label(removing.account) })}
    description={t('removeHint')}
    icon="leave"
    tone="danger"
    testid="member-remove-dialog"
  >
    <p class="font-mono text-[11.5px] text-faint">{shortId(removing.account)}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (removing = null)}>{t('cancel')}</button>
      <button
        class={buttonClass('danger')}
        disabled={busy}
        onclick={() => {
          onControl({ type: 'remove', account: removing.account })
          removing = null
        }}
        data-testid="member-remove-confirm">{t('removeMember')}</button
      >
    {/snippet}
  </Dialog>
{/if}
