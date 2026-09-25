<script lang="ts">
  import { Principal } from '@icp-sdk/core/principal'
  import type {
    CheckoutOperationsPage,
    CashTransfersPage,
    PandaOperationsPage
  } from '@dmsg/sdk'
  import { session, dateLabel } from '../session.svelte'
  import { config } from '../config'
  import { login, services } from '../services/ic'
  import { controlResult } from '../services/account'
  import { wireResult } from '../protocol/commerce'
  import { hex } from '../protocol/codec'
  let origin = $state(config.derivationOrigins[0]),
    owner = $state(''),
    api = $state.raw<Awaited<ReturnType<typeof services>> | null>(null)
  let orders = $state<CheckoutOperationsPage | null>(null),
    transfers = $state<CashTransfersPage | null>(null),
    claims = $state<PandaOperationsPage | null>(null),
    notice = $state('')
  const json = (v: unknown) =>
    JSON.stringify(
      v,
      (_, v) => (typeof v === 'bigint' ? v.toString() : v instanceof Uint8Array ? hex(v) : v),
      2
    )
  const units = (n: bigint) =>
    `${n / 1_000_000n}.${(n % 1_000_000n).toString().padStart(6, '0')}`
  async function loadOrders(after: Uint8Array | null = null) {
    const v = controlResult(await api!.commerce!.checkout_operations(after ? [after] : [], 16))
    orders = wireResult('commerce', 'checkout_operations', v) as CheckoutOperationsPage
  }
  async function loadTransfers(after: Uint8Array | null = null) {
    const v = controlResult(await api!.commerce!.checkout_transfers(after ? [after] : [], 16))
    transfers = wireResult('commerce', 'checkout_transfers', v) as CashTransfersPage
  }
  async function loadClaims(after: Uint8Array | null = null) {
    const v = controlResult(await api!.membership!.panda_operations(after ? [after] : [], 16))
    claims = wireResult('membership', 'panda_operations', v) as PandaOperationsPage
  }
  async function connect() {
    await session.run(async () => {
      if (!session.meta) throw new Error('请先解锁工作区。')
      const identity = await login(session.crypto, session.meta.transportPublic, origin, [
        config.canisters.commerce,
        config.canisters.membership
      ])
      api = await services(identity)
      owner = identity.getPrincipal().toText()
      await Promise.all([loadOrders(), loadTransfers(), loadClaims()])
    })
  }
</script>

<section class="settings-section">
  <h2>结账与承诺对账</h2>
  <p class="caption">
    从服务读取当前身份可访问的订单、转账和承诺。每个账本独立列账；查询结果用于排查，开通和到账以原操作的权威回执为准。
  </p>
  <label
    >付款人 / 商户 / 运营身份来源<select bind:value={origin}
      >{#each config.derivationOrigins as v}<option>{v}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy} onclick={connect}
    >连接并读取服务记录</button
  >
  {#if owner}<p><code>{owner}</code></p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  {#if orders}<h3>订单与分账本储备</h3>
    {#if !orders.orders.length}<p>本页没有可访问的订单。</p>{/if}
    {#each orders.orders as row}<article class="settings-section">
        <strong>{row.order.quote.offer.product_id} · {row.order.quote.offer.sku}</strong>
        <p>{row.order.progress.status} · <code>{hex(row.order.progress.order_id)}</code></p>
        {#if row.order.progress.status === 'Applying'}<p>
            Apply 结果未确定；保留原决定，不能当作失败退款。
          </p>{/if}
        {#each row.balances as b}<p>
            <code>{Principal.fromUint8Array(b.ledger).toText()}</code>
          </p>
          <dl class="evidence-list">
            <div>
              <dt>确认入账</dt>
              <dd>{units(b.incoming_atomic)}</dd>
            </div>
            <div>
              <dt>原路退款义务</dt>
              <dd>{units(b.refundable_atomic)}</dd>
            </div>
            <div>
              <dt>服务费储备</dt>
              <dd>{units(b.service_reserve_atomic)}</dd>
            </div>
            <div>
              <dt>网络费储备</dt>
              <dd>{units(b.fee_reserve_atomic)}</dd>
            </div>
            <div>
              <dt>已安排转出（含费用）</dt>
              <dd>{units(b.outgoing_atomic)}</dd>
            </div>
          </dl>{/each}
        <button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              const result = controlResult(
                await api!.commerce!.reconcile_checkout(row.order.progress.order_id)
              )
              notice = Object.keys(result.status)[0]
              await loadOrders()
            })}>对账原订单 / 决定</button
        >
        <details>
          <summary>原报价与交付回执</summary>
          <pre>{json(row.order)}</pre>
        </details>
      </article>{/each}
    {#if orders.next}<button
        class="secondary"
        disabled={session.busy}
        onclick={() => session.run(() => loadOrders(orders!.next))}>下一页订单</button
      >{/if}
  {/if}
  {#if transfers}<h3>资金转账与费用阻塞</h3>
    {#if !transfers.transfers.length}<p>
        本页没有可访问的资金转账。
      </p>{/if}{#each transfers.transfers as row}<article class="settings-section">
        <p>{row.status} · <code>{hex(row.transfer_id)}</code></p>
        <p>账本：<code>{Principal.fromUint8Array(row.ledger).toText()}</code></p>
        <p>
          收款方：<code>{Principal.fromUint8Array(row.to.owner).toText()}</code> · 金额 {units(
            row.amount_atomic
          )} · 费用 {units(row.fee_atomic)} · 原上限 {units(row.max_fee_atomic)}
        </p>
        {#if row.status === 'Unknown' || row.status === 'InFlight'}<p>
            结果未知，重试使用原 memo、时间、金额和费用。不能重建付款或清除记录。
          </p>{/if}
        {#if row.expected_fee_atomic !== null}<p>
            账本报告费用：{units(row.expected_fee_atomic)}{row.expected_fee_atomic >
            row.max_fee_atomic
              ? '，超出原批准上限，继续阻塞。'
              : ''}
          </p>{/if}
        <button
          class="secondary"
          disabled={session.busy || ['Succeeded', 'Superseded'].includes(row.status)}
          onclick={() =>
            session.run(async () => {
              const result = controlResult(
                await api!.commerce!.process_checkout_transfer(row.transfer_id)
              )
              notice = Object.keys(result.status)[0]
              await loadTransfers()
            })}>重试原转账</button
        >
        {#if row.status === 'Rejected' && row.expected_fee_atomic !== null && row.expected_fee_atomic <= row.max_fee_atomic && Principal.fromUint8Array(row.to.owner).toText() === owner}<button
            class="secondary"
            disabled={session.busy}
            onclick={() =>
              session.run(async () => {
                controlResult(
                  await api!.commerce!.revise_checkout_transfer_fee(
                    row.transfer_id,
                    row.expected_fee_atomic!
                  )
                )
                notice = '已按明确批准的费用修订已知失败的转账；总资金义务未增加。'
                await loadTransfers()
              })}>批准上述费用修订</button
          >{/if}
        <details>
          <summary>冻结的转账参数</summary>
          <pre>{json(row)}</pre>
        </details>
      </article>{/each}{#if transfers.next}<button
        class="secondary"
        disabled={session.busy}
        onclick={() => session.run(() => loadTransfers(transfers!.next))}>下一页转账</button
      >{/if}{/if}
  {#if claims}<h3>PANDA 承诺与资格租约</h3>
    {#if !claims.claims.length}<p>
        本页没有可访问的承诺。
      </p>{/if}{#each claims.claims as row}<article class="settings-section">
        <strong>{row.terms.offer.product_id} · {row.terms.offer.sku}</strong>
        <p>{row.status} · {row.eligibility} · <code>{hex(row.claim_id)}</code></p>
        <p>
          资格观察：{dateLabel(Number(row.observed_at_ms))} · 租约截止：{dateLabel(
            Number(row.valid_until_ms)
          )}
        </p>
        {#if row.valid_until_ms <= BigInt(Date.now())}<p>
            此份资格租约已过期，不能据此发放新的权益。
          </p>{/if}
        <p>
          承诺到期：{dateLabel(
            Number(row.committed_until_ms || row.terms.quote.committed_until_ms)
          )} · 抵扣全额费用 {units(row.terms.offer.amount_usd_micros)} USD
        </p>
        {#if row.status === 'Terminated'}<p>
            权益已经终止，神经元仍占用至原到期日。
          </p>{/if}<button
          class="secondary"
          disabled={session.busy}
          onclick={() =>
            session.run(async () => {
              const r = ['Checking', 'CoolingDown', 'Applying'].includes(row.status)
                ? await api!.membership!.reconcile_panda_claim(row.claim_id)
                : await api!.membership!.refresh_panda_claim(row.claim_id)
              controlResult(r)
              await loadClaims()
            })}>对账原申请 / 刷新资格</button
        >
        <details>
          <summary>原条款与回执</summary>
          <pre>{json(row)}</pre>
        </details>
      </article>{/each}{#if claims.next}<button
        class="secondary"
        disabled={session.busy}
        onclick={() => session.run(() => loadClaims(claims!.next))}>下一页承诺</button
      >{/if}<button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          const count = controlResult(await api!.membership!.sweep_panda_commitments())
          notice = `已检查 ${count} 项到期承诺。`
          await loadClaims()
        })}>处理已到期承诺</button
    >{/if}
</section>
