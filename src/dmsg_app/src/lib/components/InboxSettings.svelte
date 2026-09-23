<script lang="ts">
  import { session } from '../session.svelte'
  import { config } from '../config'
  import { login, services } from '../services/ic'
  import { AccountClient } from '../services/account'
  import { CloudClient } from '../services/relay'
  import { InboxClient } from '../services/inbox'
  import { WalletClient } from '../services/wallet'
  import { id } from '../protocol/codec'
  import type { EscrowInfo } from '../canisters/generated/payment'
  let client = $state.raw<InboxClient | null>(null),
    account = $state.raw<AccountClient | null>(null),
    wallet = $state.raw<WalletClient | null>(null)
  let origin = $state(config.derivationOrigins[0]),
    walletOrigin = $state(config.derivationOrigins[0]),
    ledger = $state(''),
    payer = $state(''),
    recipient = $state(''),
    payee = $state(''),
    net = $state('1000000')
  let mode = $state<'closed' | 'contacts' | 'free' | 'paid'>('closed'),
    allow = $state(''),
    text = $state(''),
    orderId = $state(id()),
    order = $state<any>(null),
    escrow = $state<EscrowInfo | null>(null),
    messages = $state<any[]>([]),
    outgoing = $state<{ id: string; recipient: string }[]>([]),
    status = $state('')
  const json = (value: unknown) =>
    JSON.stringify(value, (_, v) => (typeof v === 'bigint' ? v.toString() : v), 2)
  async function connect() {
    await session.run(async () => {
      if (!session.meta?.account || !config.relayOrigin || !config.canisters.payment)
        throw new Error('请先配置来信与支付服务，并绑定正式账户。')
      const identity = await login(session.crypto, session.meta.transportPublic, origin),
        api = await services(identity)
      account = new AccountClient(
        api.user!,
        api.agent,
        identity.getPrincipal(),
        session.crypto,
        session.meta,
        config.canisters.user
      )
      client = new InboxClient(
        account,
        new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
        api.payment!,
        config.canisters.payment,
        session.meta.account.id
      )
      const { Principal } = await import('@icp-sdk/core/principal')
      const terms = await client.terms()
      ledger = Principal.fromUint8Array(terms.config.ledger).toText()
      outgoing = await client.outgoing()
      status = '已认证收款路由、报价签署钥和费用政策。'
    })
  }
  async function connectWallet() {
    await session.run(async () => {
      if (!account || !session.meta || !ledger) throw new Error('先核对支付服务。')
      const identity = await login(
          session.crypto,
          session.meta.transportPublic,
          walletOrigin,
          [config.canisters.payment, ledger]
        ),
        api = await services(identity)
      wallet = new WalletClient(api.agent, identity.getPrincipal(), session.crypto)
      client = new InboxClient(
        account,
        new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
        api.payment!,
        config.canisters.payment,
        session.meta.account!.id
      )
      payer = identity.getPrincipal().toText()
      status = '付款钱包已连接，请核对实际 principal。'
    })
  }
  async function prepare() {
    await session.run(async () => {
      if (!client) throw new Error('先连接账户。')
      order = await client.contact(recipient, text, payer, orderId)
      outgoing = await client.outgoing()
      escrow = null
      status =
        order.state === 'quoted'
          ? '已保存加密内容及报价，尚未付款。'
          : '来信已按当前规则接收。'
    })
  }
  async function open() {
    await session.run(async () => {
      if (!client || !wallet || !order) throw new Error('先核对报价并连接实际付款钱包。')
      escrow = await client.openEscrow(order)
      status = '精确报价已在支付 canister 确认。'
    })
  }
  async function pay() {
    await session.run(async () => {
      if (!client || !wallet || !escrow) throw new Error('先核对已建立的托管。')
      escrow = await client.fund(escrow, wallet)
      status = '入账已确认；接收受理和最终资金决策尚需单独核对。'
    })
  }
  async function admit() {
    await session.run(async () => {
      if (!client || !escrow || !order) throw new Error('先确认托管与付款。')
      const result = await client.admit(order, escrow)
      order = result.order
      escrow = result.escrow
      status = '受理回执与 B 点资金决策已核验；实际转出以账本腿状态为准。'
    })
  }
</script>

<section class="settings-section">
  <h2>来信规则与托管投递</h2>
  <p>收款意愿、加密存储、受理回执、资金决策和实际到账分别核对。</p>
  <label
    >账户登录来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  ><button class="secondary" disabled={session.busy} onclick={connect}
    >连接并认证支付配置</button
  >
  <details>
    <summary>设置本人来信规则</summary><label
      >规则<select bind:value={mode}
        ><option value="closed">关闭</option><option value="contacts">指定联系人</option
        ><option value="free">开放免费来信</option><option value="paid">付费来信</option
        ></select
      ></label
    ><label>联系人账户（逗号分隔）<input bind:value={allow} /></label
    >{#if mode === 'paid'}<label>实际收款 principal<input bind:value={payee} /></label><label
        >接收净额（账本原子单位）<input bind:value={net} inputmode="numeric" /></label
      >
      <p>需先在设备与认证中明确启用 PaymentOffer；平台费另加，不能从净额暗扣。</p>{/if}<button
      class="primary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          await client!.configure(
            mode,
            payee,
            net,
            allow
              .split(',')
              .map((v) => v.trim())
              .filter(Boolean)
          )
          status = '规则和新的收件公钥已发布。旧收件钥仍保留在加密恢复材料中。'
        })}>批准并发布这些来信条款</button
    >
  </details>
</section>
<section class="settings-section">
  <h3>发起来信</h3>
  {#if outgoing.length}<details>
      <summary>继续已保存的投递</summary>
      {#each outgoing as pending}<button
          class="secondary"
          disabled={!client || session.busy}
          onclick={() =>
            session.run(async () => {
              order = await client!.resume(pending.id)
              orderId = pending.id
              recipient = pending.recipient
              escrow = null
              status = '已恢复原投递；核对原报价、付款身份及托管状态后继续。'
            })}>{pending.recipient} · {pending.id.slice(0, 12)}</button
        >{/each}
    </details>{/if}
  <label
    >付款身份登录来源<select bind:value={walletOrigin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  ><button class="secondary" disabled={!account || session.busy} onclick={connectWallet}
    >独立连接付款身份</button
  >{#if payer}<p>实际 payer：{payer} · 账本 {ledger}</p>{/if}
  <label>接收账户 Xid<input bind:value={recipient} maxlength="20" /></label><label
    >内容<textarea bind:value={text} maxlength="6000" rows="4"></textarea></label
  ><button
    class="secondary"
    disabled={!client || session.busy || !recipient || !text}
    onclick={prepare}>加密并核对接收方式 / 报价</button
  >
  {#if order}<p>原投递编号：{order.order_id} · 状态 {order.state}</p>
    {#if order.quote}<dl class="evidence-list">
        <div>
          <dt>接收净额</dt>
          <dd>{order.quote.recipient_net}</dd>
        </div>
        <div>
          <dt>平台费</dt>
          <dd>{order.quote.service_fee}</dd>
        </div>
        <div>
          <dt>费用准备</dt>
          <dd>{order.quote.fee_reserve}</dd>
        </div>
        <div>
          <dt>总入账金额</dt>
          <dd>{order.quote.amount}</dd>
        </div>
        <div>
          <dt>付款 / 接收截止</dt>
          <dd>
            {new Date(order.quote.fund_by).toLocaleString()} / {new Date(
              order.quote.accept_by
            ).toLocaleString()}
          </dd>
        </div>
      </dl>
      <p class="caption">
        上述金额为账本原子单位，付款网络费另外列入钱包扣款。未知结果保留原编号、密文和转账参数。
      </p>
      <button class="primary" disabled={!wallet || session.busy} onclick={open}
        >批准此精确报价并建立 / 对账托管</button
      >{/if}
    {#if escrow}<p>
        托管 {Array.from(escrow.escrow_id)
          .map((b) => b.toString(16).padStart(2, '0'))
          .join('')} · 资金决策 {Object.keys(escrow.decision)[0]} · 已确认入账 {String(
          escrow.confirmed_in
        )}
      </p>
      <button
        class="primary"
        disabled={!wallet || session.busy || !!escrow.funded_at.length}
        onclick={pay}>明确付款 / 重试原转账</button
      ><button
        class="secondary"
        disabled={session.busy || !escrow.funded_at.length}
        onclick={admit}>提交受理并确认 B 点</button
      ><button
        class="secondary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            const { hex } = await import('../protocol/codec')
            escrow = await client!.refund(hex(Uint8Array.from(escrow!.escrow_id)))
            status = '已核对超时退款决策，到账仍以转出腿为准。'
          })}>核对超时并申请退款</button
      >
      <details>
        <summary>完整托管记录</summary>
        <pre>{json(escrow)}</pre>
      </details>{/if}
    <button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          order = await client!.status(recipient, orderId)
        })}>按原投递编号对账</button
    >
    <button
      class="secondary"
      disabled={session.busy || !['visible', 'aborted'].includes(order.state)}
      onclick={() => {
        orderId = id()
        order = null
        escrow = null
        text = ''
      }}>开始新的来信</button
    >
  {/if}
</section>
<section class="settings-section">
  <h3>本人收件箱</h3>
  <button
    class="secondary"
    disabled={!client || session.busy}
    onclick={() =>
      session.run(async () => {
        messages = await client!.list()
      })}>补拉、验签并解密</button
  >{#each messages as message}<article class="history-message">
      <p>{message.sender} · {message.state}</p>
      {#if message.state === 'aborted'}<p>
          此来信未投递，内容不可读取。{message.resolved
            ? '已允许重新联系。'
            : '由你决定是否允许重新联系。'}
        </p>{:else}<p>{message.text}</p>{/if}
      <button
        class="secondary"
        disabled={session.busy || message.state === 'aborted'}
        onclick={() =>
          session.run(async () => {
            await client!.mark(message.order_id, true, false)
            messages = await client!.list()
          })}>标记已读</button
      ><button
        class="secondary"
        disabled={session.busy || message.resolved}
        onclick={() =>
          session.run(async () => {
            await client!.mark(message.order_id, true, true)
            messages = await client!.list()
          })}>{message.state === 'aborted' ? '允许重新联系' : '归档并允许新的联系'}</button
      >
    </article>{/each}
</section>
{#if status}<p role="status">{status}</p>{/if}
