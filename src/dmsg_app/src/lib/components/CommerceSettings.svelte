<script lang="ts">
  import { session, dateLabel } from '../session.svelte'
  import { config } from '../config'
  import { login, services } from '../services/ic'
  import { AccountClient } from '../services/account'
  import { CommerceClient } from '../services/commerce'
  import { WalletClient } from '../services/wallet'
  import { decodeCommerce } from '../protocol/commerce'
  import { digest, hex } from '../protocol/codec'
  import type { BillingOrder, OrderQuote, OrderAction } from '../canisters/generated/commerce'
  import type { ClaimRequest } from '../canisters/generated/membership'
  let account = $state.raw<AccountClient | null>(null),
    client = $state.raw<CommerceClient | null>(null),
    wallet = $state.raw<WalletClient | null>(null)
  let accountOrigin = $state(config.derivationOrigins[0]),
    walletOrigin = $state(config.derivationOrigins[0]),
    payer = $state(''),
    ledger = $state('')
  let catalog = $state<any>(null),
    entitlement = $state<any>(null),
    status = $state(''),
    orders = $state<Awaited<ReturnType<CommerceClient['orders']>>>([]),
    claims = $state<Awaited<ReturnType<CommerceClient['claims']>>>([])
  let selectedPlan = $state('Plus'),
    action = $state<'Subscribe' | 'Renew' | 'Upgrade'>('Subscribe'),
    selected = $state(''),
    quote = $state<OrderQuote | null>(null),
    order = $state<BillingOrder | null>(null),
    block = $state(''),
    transfer = $state('0'),
    transferBlock = $state('')
  let neuron = $state(''),
    snsPlan = $state('Plus'),
    snsJob = $state(''),
    snsRequest = $state<ClaimRequest | null>(null),
    snsStatus = $state<any>(null),
    policy = $state<any>(null),
    snsAction = $state<'Start' | 'Renew' | 'Upgrade' | 'Replace'>('Start'),
    previousClaim = $state('')
  const amount = (value: bigint, decimals: number) => {
    const scale = 10n ** BigInt(decimals),
      whole = value / scale,
      fraction = (value % scale).toString().padStart(decimals, '0').replace(/0+$/, '')
    return `${whole}${fraction ? '.' + fraction : ''}`
  }
  const json = (value: unknown) =>
    JSON.stringify(value, (_, v) => (typeof v === 'bigint' ? v.toString() : v), 2)
  async function connectAccount() {
    await session.run(async () => {
      if (!session.meta?.account || !config.canisters.commerce || !config.canisters.membership)
        throw new Error('先配置商业与共享会员服务，并绑定正式工作区。')
      const identity = await login(
          session.crypto,
          session.meta.transportPublic,
          accountOrigin
        ),
        api = await services(identity)
      account = new AccountClient(
        api.user!,
        api.agent,
        identity.getPrincipal(),
        session.crypto,
        session.meta,
        config.canisters.user
      )
      if ((await account.connectedAccount()) !== session.meta.account.id)
        throw new Error('账户不一致。')
      const preview = new CommerceClient(
        account,
        api.commerce!,
        api.membership!,
        config.canisters.commerce,
        config.canisters.membership,
        identity.getPrincipal()
      )
      const checked = await preview.catalog()
      catalog = checked.value
      ledger = checked.ledger
      entitlement = await preview.entitlement(true)
      status = '目录与权益已通过 IC 证书验证；付款身份需单独连接。'
    })
  }
  async function connectWallet() {
    await session.run(async () => {
      if (!account || !ledger || !session.meta) throw new Error('先核对目录及账本。')
      const identity = await login(
        session.crypto,
        session.meta.transportPublic,
        walletOrigin,
        [config.canisters.commerce, config.canisters.membership, ledger]
      )
      const api = await services(identity)
      client = new CommerceClient(
        account,
        api.commerce!,
        api.membership!,
        config.canisters.commerce,
        config.canisters.membership,
        identity.getPrincipal()
      )
      wallet = new WalletClient(api.agent, identity.getPrincipal(), session.crypto)
      payer = identity.getPrincipal().toText()
      orders = await client.orders()
      claims = await client.claims()
      snsJob = ''
      snsRequest = null
      policy = null
      status =
        '已连接此 Internet Identity 付款路径。SNS 资格仍由会员服务核验神经元经济控制权。'
    })
  }
  async function requestQuote() {
    await session.run(async () => {
      if (!client) throw new Error('先连接付款身份。')
      const value = { [action]: { plan: { [selectedPlan]: null } } } as OrderAction
      const result = await client.quote(value)
      selected = result.job.id
      quote = result.quote
      order = null
      orders = await client.orders()
      status = '报价已保存，尚未批准商业意图或转账。'
    })
  }
  async function showOrder(id: string) {
    await session.run(async () => {
      if (!client) throw new Error('先连接付款身份。')
      selected = id
      const job = (await client.orders()).find((v) => v.id === id)!
      quote = (decodeCommerce('commerce', 'open_order', job.input)[0] as any).quote
      if (job.stage !== 'review') order = await client.order(id)
      else order = null
    })
  }
  async function openOrder() {
    await session.run(async () => {
      if (!client) throw new Error('先连接付款身份。')
      order = await client.open(selected)
      status = '精确商业意图已批准，订单状态已认证。'
    })
  }
  async function pay() {
    await session.run(async () => {
      if (!client || !wallet || !order) throw new Error('先批准并核对订单。')
      const verified = await client.order(selected),
        reference = await wallet.transferOrder(verified)
      block = reference.toString()
      order = await client.funding(selected, block)
      entitlement = await client.entitlement(true)
      status = '已按账本记录确认付款和订单状态。'
    })
  }
  async function prepareSns() {
    await session.run(async () => {
      if (!client) throw new Error('先连接经济控制身份。')
      const plan = catalog.plans.find((p: any) => p.plan_id === snsPlan)
      if (!plan?.membership_policy_version) throw new Error('此套餐没有 SNS 政策。')
      const { unhex } = await import('../protocol/codec')
      const change =
        snsAction === 'Start'
          ? { Start: null }
          : { [snsAction]: { previous_claim: unhex(previousClaim) } }
      const result = await client.sns(
        BigInt(plan.membership_policy_version),
        neuron,
        digest('dmsg/commerce/plan/v1', plan),
        change as any
      )
      snsJob = result.job.id
      const reviewed = await client.reviewSns(snsJob)
      policy = reviewed.policy
      snsRequest = reviewed.request
      snsStatus = null
      claims = await client.claims()
    })
  }
  async function showSns(id: string) {
    await session.run(async () => {
      if (!client) throw new Error('先连接付款身份。')
      snsJob = id
      snsRequest = null
      policy = null
      snsStatus = null
      if (!id) return
      const reviewed = await client.reviewSns(id)
      policy = reviewed.policy
      snsRequest = reviewed.request
    })
  }
</script>

<section class="settings-section">
  <h2>套餐、额度与独立付款身份</h2>
  <label
    >账户登录来源<select bind:value={accountOrigin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  ><button class="secondary" disabled={session.busy} onclick={connectAccount}
    >核对商业目录与账户权益</button
  >
  {#if catalog}<p>
      已认证目录版本 {catalog.version} · 账本 {ledger} · {catalog.decimals} 位小数
    </p>
    <div class="settings-row">
      {#each catalog.plans as plan}<article>
          <strong>{plan.plan_id}</strong>
          <p>每年 {Number(plan.price_cents) / 100} USD</p>
          <p>
            {plan.limits.storage_bytes} 字节 · {plan.limits.active_channels} 个频道 · 月度 {plan
              .limits.monthly_execution_units} 执行单位
          </p>
        </article>{/each}
    </div>{/if}
  {#if entitlement}<p>
      有效套餐 {entitlement.plan_snapshot.plan_id} · 权益来源 {entitlement.source_status} · 租约至
      {dateLabel(Number(entitlement.valid_until_ms))}
    </p>
    <p>降档或关闭不会将未验证状态变成 Free；已有内容仍可按安全授权读取与导出。</p>{:else}<p>
      尚无可验证的商业状态。
    </p>{/if}
  <label
    >付款 / SNS 经济身份的原登录来源<select bind:value={walletOrigin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  ><button class="secondary" disabled={!account || session.busy} onclick={connectWallet}
    >单独连接 Internet Identity 付款路径</button
  >{#if payer}<p>
      实际付款 / SNS actor：<code>{payer}</code><br />权益接收账户：<code
        >{session.meta?.account?.id}</code
      >
    </p>{/if}
</section>
<section class="settings-section">
  <h3>现金订单</h3>
  <label
    >动作<select bind:value={action}
      ><option value="Subscribe">购买</option><option value="Renew">续费</option><option
        value="Upgrade">升级</option
      ></select
    ></label
  ><label
    >套餐<select bind:value={selectedPlan}
      ><option>Plus</option><option>Pro</option><option>Max</option></select
    ></label
  ><button class="secondary" disabled={!client || session.busy} onclick={requestQuote}
    >获取并保存精确报价</button
  >
  {#if orders.length}<label
      >原订单<select
        value={selected}
        onchange={(event) => showOrder(event.currentTarget.value)}
        ><option value="">请选择</option>{#each orders as job}<option value={job.id}
            >{job.id} · {job.stage}</option
          >{/each}</select
      ></label
    >{/if}
  {#if quote}<dl class="evidence-list">
      <div>
        <dt>商品金额</dt>
        <dd>{amount(quote.amount_atomic, quote.catalog.decimals)}</dd>
      </div>
      <div>
        <dt>费用准备</dt>
        <dd>{amount(quote.fee_reserve, quote.catalog.decimals)}</dd>
      </div>
      <div>
        <dt>本次转账网络费</dt>
        <dd>{amount(quote.catalog.ledger_fee, quote.catalog.decimals)}</dd>
      </div>
      <div>
        <dt>付款截止</dt>
        <dd>{dateLabel(Number(quote.fund_by_ms))}</dd>
      </div>
      <div>
        <dt>收款服务</dt>
        <dd>{quote.home_commerce.toText()}</dd>
      </div>
    </dl>
    <details>
      <summary>完整报价与期限</summary>
      <pre>{json(quote)}</pre>
    </details>
    <button class="primary" disabled={!client || session.busy} onclick={openOrder}
      >批准此精确商业意图并建立 / 对账原订单</button
    >{/if}
  {#if order}<p>已认证订单状态：{Object.keys(order.status)[0]}</p>
    <p>
      确认入账 {String(order.confirmed_in)} · 可退款本金 {String(order.refundable)} · 已退本金 {String(
        order.refunded_principal
      )}（账本原子单位）
    </p>
    <button
      class="primary"
      disabled={!wallet || session.busy || !('AwaitingFunding' in order.status)}
      onclick={pay}>明确支付上述金额 / 重试原转账</button
    >
    <p class="caption">
      重试保留原金额、收款子账户、memo 和时间。未知转账先对账，账本费用变化不会自动提高付款。
    </p>
    <label>原付款账本区块<input bind:value={block} inputmode="numeric" /></label><button
      class="secondary"
      disabled={!client || session.busy || !block}
      onclick={() =>
        session.run(async () => {
          order = await client!.funding(selected, block)
        })}>按原区块核对入账</button
    ><button
      class="secondary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          order = await client!.reconcile(selected)
        })}>对账原订单</button
    >
    <button
      class="secondary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          order = await client!.refund(selected)
          status = '已提交明确退款意图；到账以转账结果为准。'
        })}>批准关闭并申请退款</button
    >
    <label>转出 / 退款腿编号<input bind:value={transfer} inputmode="numeric" /></label><label
      >未知转出的原账本区块<input bind:value={transferBlock} inputmode="numeric" /></label
    ><button
      class="secondary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          const value = await client!.processTransfer(
            selected,
            transfer,
            transferBlock || undefined
          )
          status = json(value)
          order = await client!.order(selected)
        })}>继续或对账原退款腿</button
    >
    <button
      class="secondary"
      disabled={!client || session.busy || !block}
      onclick={() =>
        session.run(async () => {
          status = json(await client!.refundDeposit(selected, block))
        })}>申请该存款的未分配金额退款</button
    ><button
      class="secondary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          status = json(await client!.refundFees(selected))
        })}>申请可退费用准备</button
    >
  {/if}
</section>
<section class="settings-section">
  <h3>PANDA / SNS 资格</h3>
  <p>
    由共享会员服务核验经济控制、锁期、政策与独占。dMsg
    登录和内容权限不替代神经元资格，也不要求转交神经元管理权。
  </p>
  <label>神经元 ID（32 字节 hex）<input bind:value={neuron} maxlength="64" /></label><label
    >套餐<select bind:value={snsPlan}
      ><option>Plus</option><option>Pro</option><option>Max</option></select
    ></label
  ><label
    >资格动作<select bind:value={snsAction}
      ><option>Start</option><option>Renew</option><option>Upgrade</option><option
        >Replace</option
      ></select
    ></label
  >{#if snsAction !== 'Start'}<label
      >原 claim ID<input bind:value={previousClaim} maxlength="64" /></label
    >{/if}<button class="secondary" disabled={!client || session.busy} onclick={prepareSns}
    >核对政策并准备 claim</button
  >
  {#if claims.length}<label
      >原 claim<select
        bind:value={snsJob}
        onchange={(event) => void showSns(event.currentTarget.value)}
        ><option value="">请选择</option>{#each claims as job}<option value={job.id}
            >{job.id} · {job.stage}</option
          >{/each}</select
      ></label
    >{/if}
  {#if policy && snsRequest}<details open>
      <summary>批准前核对政策、R 与期限</summary>
      <pre>{json({
          actor: snsRequest.authorization.actor.toText(),
          neuron_id: hex(Uint8Array.from(snsRequest.neuron_id)),
          policy_version: snsRequest.policy_version,
          change: snsRequest.change,
          valid_until_ms: snsRequest.authorization.valid_until_ms
        })}</pre>
      <pre>{json(policy)}</pre>
    </details>{/if}
  <button
    class="primary"
    disabled={!client || session.busy || !snsJob || !snsRequest || !policy}
    onclick={() =>
      session.run(async () => {
        snsStatus = await client!.submitSns(snsJob)
      })}>批准并提交此精确 claim</button
  ><button
    class="secondary"
    disabled={!client || session.busy || !snsJob || !snsRequest || !policy}
    onclick={() =>
      session.run(async () => {
        snsStatus = await client!.advanceSns(snsJob)
      })}>继续原申请</button
  ><button
    class="secondary"
    disabled={!client || session.busy || !snsJob || !snsRequest || !policy}
    onclick={() =>
      session.run(async () => {
        snsStatus = await client!.refreshSns(snsJob)
      })}>刷新资格 / 冷却 / 修复状态</button
  ><button
    class="secondary"
    disabled={!client || session.busy || !snsJob || !snsRequest || !policy}
    onclick={() =>
      session.run(async () => {
        snsStatus = await client!.reconcileSns(snsJob)
      })}>对账原 claim</button
  ><button
    class="secondary"
    disabled={!client || session.busy || !snsJob || !snsRequest || !policy}
    onclick={() =>
      session.run(async () => {
        snsStatus = await client!.closeSns(snsJob)
      })}>批准关闭此 claim</button
  >
  {#if snsStatus}<pre>{json(snsStatus)}</pre>{/if}
</section>
{#if status}<p role="status">{status}</p>{/if}
