<script lang="ts">
  import { session } from '../session.svelte'
  import { hex } from '../protocol/codec'
  import type { PaymentClient } from '../services/payment'
  import type {
    Account,
    Deposit,
    EscrowInfo,
    RefundQuote,
    TransferLeg
  } from '../canisters/generated/payment'

  let { client, escrow }: { client: PaymentClient; escrow: EscrowInfo } = $props()
  let deposits = $state<Deposit[]>([]),
    transfers = $state<TransferLeg[]>([]),
    selected = $state<bigint[]>([]),
    reserve = $state(false),
    preview = $state<RefundQuote | null>(null),
    depositCursor = $state<bigint | undefined>(),
    transferCursor = $state<bigint | undefined>(),
    moreDeposits = $state(false),
    moreTransfers = $state(false),
    loadedEscrow = $state('')
  let reconcileBlock = $state('')
  const escrowId = $derived(hex(Uint8Array.from(escrow.escrow_id)))
  const address = (account: Account) =>
    `${account.owner.toText()}${account.subaccount[0] ? ` / ${hex(Uint8Array.from(account.subaccount[0]))}` : ''}`

  $effect(() => {
    if (loadedEscrow !== escrowId) {
      loadedEscrow = escrowId
      deposits = []
      transfers = []
      selected = []
      reserve = false
      preview = null
      depositCursor = undefined
      transferCursor = undefined
      moreDeposits = false
      moreTransfers = false
    }
  })

  async function loadDeposits(next = false) {
    const page = await client.deposits(escrowId, next ? depositCursor : undefined)
    deposits = page
    depositCursor = page.at(-1)?.block
    moreDeposits = page.length === 32
    if (!next) selected = []
    preview = null
  }

  async function loadTransfers(next = false) {
    const page = await client.transfers(escrowId, next ? transferCursor : undefined)
    transfers = page
    transferCursor = page.at(-1)?.leg_id
    moreTransfers = page.length === 32
  }

  async function refresh() {
    await loadDeposits()
    await loadTransfers()
  }

  function choose(block: bigint, checked: boolean) {
    selected = checked ? [...selected, block] : selected.filter((id) => id !== block)
    preview = null
  }

  async function claim() {
    const transfer = await client.claimRefund(escrowId, selected, reserve)
    preview = null
    selected = []
    // The transfer is already durable. Keep it visible even when execution fails.
    transfers = [transfer, ...transfers.filter((item) => item.leg_id !== transfer.leg_id)]
    await loadDeposits()
  }

  async function send(transfer: TransferLeg) {
    try {
      const result = await client.processTransfer(escrowId, transfer.leg_id)
      transfers = transfers.map((item) => (item.leg_id === result.leg_id ? result : item))
    } catch (error) {
      await loadTransfers()
      throw error
    }
  }

  async function revise(transfer: TransferLeg) {
    const result = await client.reviseTransfer(
      escrowId,
      transfer.leg_id,
      transfer.expected_fee[0] ?? transfer.fee
    )
    await loadTransfers()
    transfers = [result, ...transfers.filter((item) => item.leg_id !== result.leg_id)]
  }
</script>

<details>
  <summary>入金、退款与转账恢复</summary>
  <p class="caption">
    金额均为账本原子单位。退款固定回原出资账户，可合并同一账户的多笔余额；资金决策不代表已到账。
  </p>
  <button class="secondary" disabled={session.busy} onclick={() => session.run(refresh)}
    >刷新资金记录</button
  >
  {#each deposits as deposit (deposit.block)}
    <label>
      <input
        type="checkbox"
        checked={selected.includes(deposit.block)}
        disabled={session.busy ||
          deposit.refundable === 0n ||
          (selected.length >= 32 && !selected.includes(deposit.block))}
        onchange={(event) => choose(deposit.block, event.currentTarget.checked)}
      />
      入金 #{String(deposit.block)} · 待分配退款 {String(deposit.refundable)} · {address(
        deposit.from
      )}
    </label>
  {/each}
  {#if moreDeposits}<button
      class="secondary"
      disabled={session.busy}
      onclick={() => session.run(() => loadDeposits(true))}>下一页入金</button
    >{/if}
  {#if selected.length}<p>
      已选择 {selected.length}/32 笔入金：{selected.map(String).join('、')}
    </p>{/if}
  <label>
    <input
      type="checkbox"
      bind:checked={reserve}
      disabled={session.busy || 'Pending' in escrow.decision}
      onchange={() => (preview = null)}
    />
    合并已释放的原始托管余额或费用预留（返还 {address(escrow.quote.payer)}）
  </label>
  <p class="caption">原始托管余额在退款决定后释放；结算的费用余量在全部收款转账完成后释放。</p>
  <button
    class="secondary"
    disabled={session.busy || (!selected.length && !reserve)}
    onclick={() =>
      session.run(async () => {
        preview = await client.refundQuote(escrowId, selected, reserve)
      })}>核对可退金额</button
  >
  {#if preview}
    <p>
      所选余额 {String(preview.available)} · 当前手续费 {String(preview.fee)} · 实际可退 {String(
        preview.amount
      )} · 收款账户 {address(preview.to)}
    </p>
    {#if preview.amount === 0n}
      <p>
        {preview.available > 0n
          ? '余额暂不足以支付退款手续费，资金仍属于原出资账户，可与同源款项合并领取。'
          : '所选款项已分配或没有可领取余额，请查看转账记录。'}
      </p>
    {:else}
      <button class="primary" disabled={session.busy} onclick={() => session.run(claim)}
        >准备退款转账</button
      >
    {/if}
  {/if}
  <label>原转出账本区块<input bind:value={reconcileBlock} inputmode="numeric" /></label>
  {#each transfers as transfer (transfer.leg_id)}
    <p>
      转账 #{String(transfer.leg_id)} · {Object.keys(transfer.status)[0]} · 金额 {String(
        transfer.amount
      )} + 手续费 {String(transfer.fee)} · {address(transfer.to)}
    </p>
    {#if 'Unknown' in transfer.status || 'InFlight' in transfer.status}<button
        class="secondary"
        disabled={session.busy || !/^[0-9]+$/.test(reconcileBlock)}
        onclick={() =>
          session.run(async () => {
            await client.reconcileTransfer(escrowId, transfer.leg_id, BigInt(reconcileBlock))
            await loadTransfers()
          })}>按原区块核对转出</button
      >{/if}
    {#if !('Succeeded' in transfer.status || 'Superseded' in transfer.status)}
      <button
        class="secondary"
        disabled={session.busy}
        onclick={() => session.run(() => send(transfer))}>执行 / 原参数重试</button
      >
      {#if 'Rejected' in transfer.status || ('FeeBlocked' in transfer.status && transfer.expected_fee.length)}
        <button
          class="secondary"
          disabled={session.busy}
          onclick={() => session.run(() => revise(transfer))}>按已确认拒绝结果修订</button
        >
      {/if}
    {/if}
  {/each}
  {#if moreTransfers}<button
      class="secondary"
      disabled={session.busy}
      onclick={() => session.run(() => loadTransfers(true))}>下一页转账</button
    >{/if}
</details>
