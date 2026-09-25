<script lang="ts">
  import { Principal } from '@icp-sdk/core/principal'
  import {
    canonical,
    type CheckoutRequest,
    type CheckoutView,
    type PandaClaimView,
    type CashTransfer
  } from '@dmsg/sdk'
  import { session, dateLabel } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount as accountConnection, connectIdentity } from '../connection'
  import type { AccountClient } from '../services/account'
  import { CommerceClient, type CheckoutJob } from '../services/commerce'
  import { WalletClient } from '../services/wallet'
  import { b64, hex } from '../protocol/codec'
  let {
    request = null,
    origin = location.origin,
    beforeAuthorize = async () => {},
    onStatus = async (_value: CheckoutView | PandaClaimView) => {}
  }: {
    request?: CheckoutRequest | null
    origin?: string
    beforeAuthorize?: () => Promise<void>
    onStatus?: (v: CheckoutView | PandaClaimView) => Promise<void>
  } = $props()
  let account = $state.raw<AccountClient | null>(null),
    client = $state.raw<CommerceClient | null>(null),
    wallet = $state.raw<WalletClient | null>(null)
  let accountOrigin = $state(config.derivationOrigins[0]),
    walletOrigin = $state(config.derivationOrigins[0]),
    method = $state<'Cash' | 'Panda'>('Cash'),
    sku = $state('plus'),
    ledger = $state(''),
    neuron = $state('')
  let assets = $state<Awaited<ReturnType<CommerceClient['assets']>>>([]),
    catalog = $state<any>(null),
    entitlement = $state<any>(null),
    jobs = $state<CheckoutJob[]>([]),
    job = $state<CheckoutJob | null>(null),
    view = $state<CheckoutView | PandaClaimView | null>(null)
  let refundLedger = $state('')
  let block = $state(''),
    refundBlocks = $state(''),
    transferId = $state(''),
    transferBlock = $state(''),
    revisedFee = $state(''),
    transferResult = $state<unknown>(null)
  let selectedMethod = $derived(request?.method ?? method)
  let terms = $derived(job && client ? client.terms(job) : null)
  const json = (v: unknown) =>
    JSON.stringify(
      v,
      (_, x) => (typeof x === 'bigint' ? x.toString() : x instanceof Uint8Array ? hex(x) : x),
      2
    )
  const units = (n: bigint, decimals = 6) => {
    const s = 10n ** BigInt(decimals)
    return `${n / s}.${(n % s).toString().padStart(decimals, '0').replace(/0+$/, '') || '0'}`
  }
  async function connectAccount() {
    await session.run(async () => {
      if (!session.meta?.account) throw new Error('请先解锁已注册的 dMsg 账户。')
      const { identity, api, account: connected } = await accountConnection(accountOrigin)
      account = connected
      const preview = new CommerceClient(
        account,
        api.commerce!,
        api.membership!,
        config.canisters.commerce,
        config.canisters.membership,
        identity.getPrincipal()
      )
      assets = (await preview.assets()).filter((v) => v.ledger_verified && v.policy.enabled)
      ledger = assets.length ? Principal.fromUint8Array(assets[0].policy.ledger).toText() : ''
      if (!request) {
        catalog = await preview.catalog()
        entitlement = await preview.entitlement(true)
      }
    })
  }
  async function connectWallet() {
    await session.run(async () => {
      if (!account || !session.meta) throw new Error('请先连接 dMsg 账户。')
      const { identity, api } = await connectIdentity(walletOrigin, [
        config.canisters.commerce,
        config.canisters.membership,
        ...assets.map((v) => Principal.fromUint8Array(v.policy.ledger).toText())
      ])
      client = new CommerceClient(
        account,
        api.commerce!,
        api.membership!,
        config.canisters.commerce,
        config.canisters.membership,
        identity.getPrincipal()
      )
      wallet = new WalletClient(api.agent, identity.getPrincipal(), session.crypto)
      jobs = (await client.jobs()).filter(
        (j) =>
          !request ||
          hex(client!.request(j).offer.operation_id) === hex(request.offer.operation_id)
      )
      if (jobs.length) {
        job = jobs[jobs.length - 1]
        if (job.stage !== 'review') {
          try {
            view = await client.status(job.id)
            await onStatus(view)
          } catch {
            /* A saved approval may not have opened the service operation yet. */
          }
        }
      }
    })
  }
  async function quote() {
    await session.run(async () => {
      if (!client) throw new Error('请连接付款 / 神经元经济身份。')
      const input = request ?? (await client.personal(sku, method))
      job = await client.quote(input, origin, selectedMethod === 'Cash' ? ledger : neuron)
      view = null
      jobs = await client.jobs()
    })
  }
  async function update(
    action: (c: CommerceClient, id: string) => Promise<CheckoutView | PandaClaimView>
  ) {
    await session.run(async () => {
      if (!client || !job) throw new Error('请选择原操作。')
      try {
        view = await action(client, job.id)
        await onStatus(view)
      } finally {
        job = await client.job(job.id)
      }
    })
  }
  async function approve(fresh = false) {
    await update((c, id) => c.approve(id, fresh, beforeAuthorize))
  }
  async function transfer(action: (c: CommerceClient, id: string) => Promise<CashTransfer>) {
    await session.run(async () => {
      if (!client || !job) throw new Error('请选择原订单。')
      const t = await action(client, job.id)
      transferId = hex(t.transfer_id)
      transferResult = t
    })
  }
</script>

<section class="settings-section">
  <h2>订阅与付款</h2>
  {#if request}<p>请求来源：<code>{origin}</code></p>
    <p>
      {request.offer.product_id} · {request.offer.sku} · {units(
        request.offer.amount_usd_micros
      )} USD
    </p>
    <p>
      订阅区间：{dateLabel(Number(request.offer.starts_at_ms))} — {dateLabel(
        Number(request.offer.expires_at_ms)
      )}
    </p>
    <p>
      受益主体：<code
        >{request.offer.beneficiary.subject_schema}:{hex(
          request.offer.beneficiary.subject_bytes
        )}</code
      >
    </p>{/if}
  <label
    >账户登录来源<select bind:value={accountOrigin}
      >{#each config.derivationOrigins as v}<option>{v}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy} onclick={connectAccount}
    >连接 dMsg 账户并核对配置</button
  >
  {#if entitlement}<p>
      当前套餐：{entitlement.plan_snapshot.plan_id} · 权益来源：{entitlement.source_status} · 租约至
      {dateLabel(Number(entitlement.valid_until_ms))}
    </p>{/if}
  <label
    >付款 / SNS 经济身份来源<select bind:value={walletOrigin}
      >{#each config.derivationOrigins as v}<option>{v}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={!account || session.busy} onclick={connectWallet}
    >连接付款 / 神经元经济身份</button
  >
  {#if client}<p>经济身份：<code>{client.wallet.toText()}</code></p>
    <p>dMsg 账户：<code>{session.meta?.account?.id}</code></p>{/if}
  {#if !request}<label
      >订阅方式<select bind:value={method}
        ><option value="Cash">ckUSDT / ckUSDC 付款</option><option value="Panda"
          >PANDA 全额抵扣</option
        ></select
      ></label
    ><label
      >套餐<select bind:value={sku}
        ><option value="plus">Plus</option><option value="pro">Pro</option><option value="max"
          >Max</option
        >{#if method === 'Cash'}<option value="upgrade-pro">现金升级至 Pro</option><option
            value="upgrade-max">现金升级至 Max</option
          >{#each catalog?.storage_products ?? [] as item}<option value={hex(item.product_id)}
              >存储增购 · {item.storage_bytes} 字节</option
            >{/each}{/if}</select
      ></label
    >{/if}
  {#if selectedMethod === 'Cash'}<label
      >付款资产<select bind:value={ledger}
        >{#each assets as v}<option value={Principal.fromUint8Array(v.policy.ledger).toText()}
            >{v.policy.asset === 'CkUsdc' ? 'ckUSDC' : 'ckUSDT'}</option
          >{/each}</select
      ></label
    >
    <p class="caption">账本：<code>{ledger || '尚无可用资产'}</code></p>{:else}<label
      >SNS 神经元 ID<input
        bind:value={neuron}
        maxlength="64"
        placeholder="64 位十六进制"
      /></label
    >
    <p>
      使用神经元全额抵扣，现金订阅费为零。首次合格观察后至少冷却 65
      分钟，随后需要重新批准。成功开通后，包括尚未开始的续费，承诺均持续到原到期日，不能提前退出、替换或现金买断。SNS
      原生控制权仍属于你。
    </p>{/if}
  <button
    class="secondary"
    disabled={!client || session.busy || (!!job && job.stage !== 'review')}
    onclick={quote}>获取并保存精确报价</button
  >
  {#if jobs.length}<label
      >原操作<select
        value={job?.id ?? ''}
        onchange={(e) =>
          session.run(async () => {
            job = await client!.job(e.currentTarget.value)
            view = job.stage === 'review' ? null : await client!.status(job.id)
          })}
        >{#each jobs as j}<option value={j.id}>{j.id.slice(0, 12)} · {j.stage}</option
          >{/each}</select
      ></label
    >{/if}
  {#if terms && job}<dl class="evidence-list">
      {#if 'cash' in terms}<div>
          <dt>商品金额</dt>
          <dd>{units(terms.cash.amount_atomic)}</dd>
        </div>
        <div>
          <dt>网络费储备</dt>
          <dd>{units(terms.cash.fee_reserve_atomic)}</dd>
        </div>
        <div>
          <dt>本次付款网络费</dt>
          <dd>{units(terms.asset.network_fee_atomic)}</dd>
        </div>
        <div>
          <dt>付款截止</dt>
          <dd>{dateLabel(Number(terms.cash.funding_deadline_ms))}</dd>
        </div>{:else}<div>
          <dt>最低 PANDA 质押</dt>
          <dd>{units(terms.quote.required_stake_e8s, 8)}</dd>
        </div>
        <div>
          <dt>现金订阅费</dt>
          <dd>0</dd>
        </div>
        <div>
          <dt>不可缩短的承诺到期日</dt>
          <dd>{dateLabel(Number(terms.quote.committed_until_ms))}</dd>
        </div>{/if}
    </dl>
    <details>
      <summary>完整报价、接收方与期限</summary>
      <pre>{json(terms)}</pre>
    </details>
    {#if !view}<button class="primary" disabled={session.busy} onclick={() => approve()}
        >批准上述精确条款 / 重送原批准</button
      >{/if}
    <button
      class="secondary"
      disabled={session.busy || job.stage === 'review'}
      onclick={() => update((c, id) => c.reconcile(id))}>读取并对账原操作</button
    >
  {/if}
  {#if view}<p>服务状态：{'progress' in view ? view.progress.status : view.status}</p>
    {#if 'progress' in view}
      {#if view.progress.status === 'AwaitingFunding'}<button
          class="primary"
          disabled={session.busy || !wallet}
          onclick={() =>
            update(async (c, id) => {
              const current = (await c.status(id)) as CheckoutView
              const b = await wallet!.transferCheckout(current)
              block = b.toString()
              return c.funding(id, block)
            })}>明确付款 / 重试原转账</button
        >{/if}
      <p class="caption">
        未知转账保留原 memo、时间、收款子账户和费用；不会自动创建第二笔付款。
      </p>
      <label>原付款区块<input bind:value={block} inputmode="numeric" /></label><button
        class="secondary"
        disabled={session.busy || !block}
        onclick={() => update((c, id) => c.funding(id, block))}>核对原账本入账</button
      >
      {#if ['AwaitingFunding', 'Applied'].includes(view.progress.status)}<button
          class="secondary"
          disabled={session.busy}
          onclick={() => update((c, id) => c.cancel(id))}>取消未开始的现金订阅</button
        >{/if}
      <label
        >退款所属账本<select bind:value={refundLedger}
          ><option value="">订单付款账本</option>{#each assets as asset}<option
              value={Principal.fromUint8Array(asset.policy.ledger).toText()}
              >{asset.policy.asset}</option
            >{/each}</select
        ></label
      >
      <label>退款入账区块（逗号分隔）<input bind:value={refundBlocks} /></label><button
        class="secondary"
        disabled={session.busy || !refundBlocks}
        onclick={() =>
          transfer((c, id) =>
            c.refund(
              id,
              refundBlocks.split(',').map((v) => v.trim()),
              refundLedger || undefined
            )
          )}>原路退款至实际付款账户</button
      >
      <button
        class="secondary"
        disabled={session.busy}
        onclick={() => transfer((c, id) => c.refundFees(id))}>领取未使用网络费储备</button
      ><button
        class="secondary"
        disabled={session.busy}
        onclick={() => transfer((c, id) => c.collect(id))}>商户领取已赚取收入</button
      >
      <label>服务转账 ID<input bind:value={transferId} /></label><label
        >原转账区块（对账时填写）<input
          bind:value={transferBlock}
          inputmode="numeric"
        /></label
      ><button
        class="secondary"
        disabled={session.busy || !transferId}
        onclick={() =>
          session.run(async () => {
            transferResult = await client!.processTransfer(
              transferId,
              transferBlock || undefined
            )
          })}>执行 / 对账原服务转账</button
      >
      <label
        >明确批准的新网络费（原子单位）<input
          bind:value={revisedFee}
          inputmode="numeric"
        /></label
      ><button
        class="secondary"
        disabled={session.busy || !transferId || !revisedFee}
        onclick={() =>
          session.run(async () => {
            const revised = await client!.reviseFee(transferId, revisedFee)
            transferResult = revised
            transferId = hex(revised.transfer_id)
          })}>仅对已知失败的转账修订费用</button
      >
    {:else}
      <p>
        冷却截止：{view.cooling_until_ms
          ? dateLabel(Number(view.cooling_until_ms))
          : '等待首次合格观察'} · 资格：{json(view.eligibility)} · 租约至 {dateLabel(
          Number(view.valid_until_ms)
        )}
      </p>
      {#if view.status === 'CoolingDown'}<button
          class="primary"
          disabled={session.busy || Number(view.cooling_until_ms) > Date.now()}
          onclick={() => approve(true)}>冷却后重新批准原承诺并开通</button
        >{/if}
      <button
        class="secondary"
        disabled={session.busy}
        onclick={() => update((c, id) => c.refresh(id))}>刷新资格和原承诺</button
      >
      {#if ['Checking', 'CoolingDown'].includes(view.status)}<button
          class="secondary"
          disabled={session.busy}
          onclick={() => update((c, id) => c.cancel(id))}>取消尚未 Apply 的申请</button
        >{/if}
      {#if view.committed_until_ms > 0n}<p>
          神经元占用持续至 {dateLabel(
            Number(view.committed_until_ms)
          )}。资格失效或权益终止不会提前释放。
        </p>{/if}
    {/if}
    {#if !request && ('progress' in view ? ['Applied', 'RefundCommitted', 'Rejected'].includes(view.progress.status) : ['Active', 'Terminated', 'Cancelled', 'Rejected', 'Released'].includes(view.status))}<button
        class="secondary"
        onclick={() => {
          job = null
          view = null
        }}>准备下一笔独立订阅</button
      >{/if}
    <details>
      <summary>服务记录</summary>
      <pre>{json(view)}</pre>
    </details>
    <button
      class="secondary"
      onclick={() => {
        const a = document.createElement('a')
        a.href = URL.createObjectURL(new Blob([b64(canonical(view))], { type: 'text/plain' }))
        a.download = 'dmsg-checkout.cbor.base64'
        a.click()
        URL.revokeObjectURL(a.href)
      }}>导出原服务记录</button
    >
  {/if}
  {#if transferResult}<pre>{json(transferResult)}</pre>{/if}
</section>
