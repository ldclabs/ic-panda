<script lang="ts">
  import { walletError as errorText } from './errors'
  import { onMount, untrack } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Field from '../../components/page/Field.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Select from '../../components/Select.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import { buttonClass } from '../../components/ui'
  import { call } from '../../rpc'
  import { t } from '../../i18n/index.svelte'
  import { isMemberId } from './new-wallet'
  import { assetIdentity, assetKey, policyDraft, sameCosign, cosignerReferences, removedMemberKeys, unreachableCosigners, utf8Length, type Asset, type Wallet } from './model'
  import Policy from './Policy.svelte'
  import { w } from './text'
  let {
    open = $bindable(false),
    wallet,
    metadata,
    kind,
    target,
    onDone,
    onSetup
  }: { open?: boolean; wallet: Wallet; metadata: Asset[]; kind: string; target?: any; onDone: (p: any) => void; onSetup: () => void } = $props()
  let alive = true
  onMount(() => () => {
    alive = false
  })
  let error = $state(''),
    busy = $state(false)
  let id = $state(untrack(() => target?.id ?? '')),
    role = $state(untrack(() => target?.role ?? 'Approver')),
    label = $state(untrack(() => target?.label ?? '')),
    agents = $state(untrack(() => (target?.agent_ids ?? []).join('\n')))
  let policy = $state(untrack(() => policyDraft(wallet.policy)))
  let chain = $state('Icp'),
    segwit = $state(false),
    asset = $state(
      untrack(
        () =>
          metadata.find(
            (a) =>
              !target ||
              (
                { ICP: 'Icp', ICRC: 'Icp', BTC: 'Bitcoin', ETH: 'Evm', ERC20: 'Evm', SOL: 'Solana', SPL: 'Solana' } as Record<
                  string,
                  string
                >
              )[a.asset] === target.chain
          )?.key ?? 'icp'
      )
    ),
    destination = $state(untrack(() => target?.destination ?? '')),
    amount = $state('1'),
    from = $state('0')
  let selected = $state<number[]>(untrack(() => [...(target?.cosign?.cosigners ?? [])])),
    threshold = $state(untrack(() => target?.cosign?.threshold ?? 1))
  let confirmed = $state(false)
  let removeCosign = $state(false)
  const candidate = $derived(removeCosign ? null : { cosigners: selected, threshold })
  const unchanged = $derived(kind === 'cosign' && sameCosign(target?.cosign, candidate))
  const removedIds = $derived(kind === 'remove_cosigner' ? [target.id]
    : kind === 'remove_member' ? removedMemberKeys(wallet, target.id) : [])
  const references = $derived(cosignerReferences(wallet, removedIds))
  const unreachable = $derived(kind === 'policy' ? unreachableCosigners(wallet, policy.hardware_cosigners) : [])
  const titles = $derived<Record<string, string>>({
    member: target ? 'editMember' : 'addMember',
    remove_member: 'remove_member',
    policy: 'editPolicy',
    book: 'addBook',
    book_remove: 'book_remove',
    address: 'addAddress',
    cosign: 'bindCosign',
    refuel: 'refuel',
    remove_cosigner: 'remove_cosigner',
    lock_code: 'lockCode',
    unfreeze: 'unfreeze'
  })
  const invalid = $derived(
    (kind === 'member' && (!isMemberId(id.trim()) || utf8Length(label) > 64)) ||
      (kind === 'address' && (!label.trim() || utf8Length(label) > 64)) ||
      (kind === 'lock_code' && !confirmed) ||
      references.length > 0 || unreachable.length > 0 ||
      (kind === 'cosign' && (unchanged || (removeCosign ? !target?.cosign :
        selected.length === 0 || selected.length > 16 || !Number.isInteger(threshold) || threshold < 1 || threshold > selected.length ||
        selected.some((id) => !wallet.cosigners.some((c) => c.id === id && (!wallet.policy.hardware_cosigners || c.hardware))))))
  )
  async function submit() {
    if (invalid) return
    busy = true
    error = ''
    try {
      let change: any
      switch (kind) {
        case 'member':
          change = { kind, id: id.trim(), role, label: label.trim(), agent_ids: agents.split(/\s+/).filter(Boolean), update: !!target }
          break
        case 'remove_member':
          change = { kind, id: target.id }
          break
        case 'policy':
          change = { kind, policy }
          break
        case 'book':
        case 'book_remove':
          change = { kind: 'book', asset, destination: destination.trim(), label: label.trim(), remove: kind === 'book_remove' }
          break
        case 'address':
          change = { kind, chain, label: label.trim(), segwit: chain === 'Bitcoin' && segwit, cosign: null }
          break
        case 'cosign':
          change = { kind, address: target.id, cosign: candidate }
          break
        case 'refuel':
          change = { kind, from: Number(from), amount }
          break
        case 'remove_cosigner':
          change = { kind, id: target.id }
          break
        default:
          change = { kind }
      }
      const proposal = await call('wallet/change', { wallet: wallet.wallet, change })
      if (!alive) return
      onDone(proposal)
      open = false
    } catch (e) {
      error = errorText(e)
    } finally {
      busy = false
    }
  }
</script>

<Dialog bind:open title={w(titles[kind] ?? kind)} wide={kind === 'policy'} icon="coins" testid="wallet-manage">
  <div class="grid gap-4">
    {#if kind === 'member'}
      <Field label={t('twMemberId')}
        ><input
          bind:value={id}
          disabled={!!target}
          dir="ltr"
          aria-invalid={id.length > 0 && !isMemberId(id.trim())}
          data-testid="wallet-edit-member-id"
        />{#if id && !isMemberId(id.trim())}<small class="text-danger">{w('invalidId')}</small>{/if}</Field
      >
      <Field label={w('role')}
        ><Select
          bind:value={role}
          options={['Admin', 'Approver', 'Proposer', 'Viewer', 'Recovery'].map((r) => ({ value: r, label: w(r) }))}
        /></Field
      >
      <Field label={w('label')}><input bind:value={label} /></Field>
      <Field label={w('agent_ids')}><textarea bind:value={agents} rows="3" dir="ltr" class="font-mono"></textarea></Field>
      {#if role === 'Recovery'}<Notice>{w('recoveryHint')}</Notice>{/if}
    {:else if kind === 'policy'}<Policy bind:value={policy} {metadata} />
    {:else if kind === 'book' || kind === 'book_remove'}
      <Field label={w('asset')}
        ><Select
          bind:value={asset}
          options={metadata
            .filter(
              (a) =>
                !target ||
                target.chain ===
                  (
                    { ICP: 'Icp', ICRC: 'Icp', BTC: 'Bitcoin', ETH: 'Evm', ERC20: 'Evm', SOL: 'Solana', SPL: 'Solana' } as Record<
                      string,
                      string
                    >
                  )[a.asset]
            )
            .map((a) => ({ value: assetKey(a), label: assetIdentity(a) }))}
        /></Field
      >
      <Field label={w('recipient')}><input bind:value={destination} disabled={kind === 'book_remove'} dir="ltr" /></Field>
      {#if kind === 'book'}<Field label={w('label')}><input bind:value={label} /></Field>{/if}
    {:else if kind === 'address'}
      <Field label={w('chain')}
        ><Select
          bind:value={chain}
          options={['Icp', 'Bitcoin', 'Evm', 'Solana'].map((c) => ({ value: c, label: c === 'Icp' ? 'ICP' : c === 'Evm' ? 'EVM' : c }))}
        /></Field
      >
      <Field label={w('label')}><input bind:value={label} /></Field>
      {#if chain === 'Bitcoin'}<Checkbox bind:checked={segwit}>Segwit · P2WPKH</Checkbox>{/if}
    {:else if kind === 'cosign'}
      <p class="text-[12px]">#{target.id} · {target.label} · {target.chain}</p>
      <p class="font-mono text-[12px] break-all" dir="ltr">{target.text}</p>
      {#if target.cosign}<Checkbox bind:checked={removeCosign}>{w('cosignRemove')}</Checkbox>{/if}
      {#if removeCosign}<Notice tone="warn">{w('cosignRemoveHint')}</Notice>
      {:else if !wallet.cosigners.length}<Notice>{w('cosignSetupHint')}
        <button class={buttonClass('secondary', 'sm', 'mt-2')} onclick={onSetup}>{w('settings')}</button></Notice>
      {:else}
      {#each wallet.cosigners as c (c.id)}<Checkbox
          checked={selected.includes(c.id)}
          disabled={(wallet.policy.hardware_cosigners && !c.hardware) || (!selected.includes(c.id) && selected.length >= 16)}
          onCheckedChange={(on) => (selected = on ? [...selected, c.id] : selected.filter((id) => id !== c.id))}
          >#{c.id} · {c.label} · {c.member} · {c.hardware ? w('hardware') : w('softwareWarning')}</Checkbox
        >{/each}
      <Field label={w('threshold')}><input type="number" min="1" max={selected.length || 1} bind:value={threshold} /></Field>
      <p class="text-faint text-[12px]">{w(unchanged ? 'cosignUnchanged' : 'cosignSelect')}</p>
      {/if}
    {:else if kind === 'refuel'}
      <Field label={w('from')}
        ><Select
          bind:value={from}
          options={wallet.addresses.filter((a) => a.chain === 'Icp').map((a) => ({ value: String(a.id), label: `#${a.id} · ${a.label}` }))}
        /></Field
      >
      <Field label={`${w('amount')} · ICP`}><input bind:value={amount} inputmode="decimal" dir="ltr" /></Field>
    {:else if kind === 'lock_code'}
      <Notice tone="danger">{w('lockHint')}</Notice><Checkbox bind:checked={confirmed}>{w('lockCode')}</Checkbox>
    {:else if kind === 'unfreeze'}<Notice>{w('freezeHint')}</Notice>
    {:else}<p class="font-mono text-[12px] break-all">{target?.label} · {target?.id}</p>{/if}
    {#if utf8Length(label) > 64}<Notice tone="danger">{w('nameBytes')}</Notice>{/if}
    {#if references.length}<Notice tone="warn">{w('cosignBound')}
      {#each references as a (a.id)}<p>#{a.id} · {a.label} · {a.text}</p>{/each}</Notice>{/if}
    {#if unreachable.length}<Notice tone="warn">{w('cosignHardwareUnreachable')}
      {#each unreachable as a (a.id)}<p>#{a.id} · {a.label} · {a.cosign.threshold} / {a.cosign.cosigners.length}</p>{/each}</Notice>{/if}
    {#if error}<Notice tone="danger">{error}</Notice>{/if}
  </div>
  {#snippet footer()}<span class="flex-1"></span><button class={buttonClass('ghost')} onclick={() => (open = false)}>{t('cancel')}</button
    ><button class={buttonClass('primary')} disabled={busy || invalid} onclick={submit} data-testid="wallet-manage-submit"
      >{w('submit')}</button
    >{/snippet}
</Dialog>
