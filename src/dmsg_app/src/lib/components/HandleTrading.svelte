<script lang="ts">
  import { session } from '../session.svelte'
  import { config } from '../config'
  import { connectAccount, connectIdentity, loginOrigin } from '../connection'
  import type { AccountClient } from '../services/account'
  import { HandleClient } from '../services/handle'
  import { WalletClient } from '../services/wallet'
  import { decodeHandle } from '../protocol/handle'
  import { xidText } from '../protocol/identity'
  let origin = $state(loginOrigin()),
    payerOrigin = $state(config.derivationOrigins[0])
  let account = $state.raw<AccountClient | null>(null),
    client = $state.raw<HandleClient | null>(null),
    wallet = $state.raw<WalletClient | null>(null)
  let name = $state(''),
    ledger = $state(''),
    target = $state(''),
    packet = $state(''),
    block = $state(''),
    status = $state('')
  let job = $state<Awaited<ReturnType<HandleClient['purchaseJob']>>>(null),
    review = $state<ReturnType<HandleClient['inspectTransfer']> | null>(null)
  async function connect() {
    await session.run(async () => {
      const c = await connectAccount(origin)
      if (!c.api.handle) throw new Error('尚未配置名称服务。')
      account = c.account
      client = new HandleClient(
        account,
        c.api.handle,
        config.canisters.handle,
        c.identity.getPrincipal()
      )
      ledger = (await c.api.handle.get_handle_config()).ledger.toText()
      wallet = null
      const saved = await account.crypto.call(
        'controlGet',
        `handle-transfer:${account.meta.account!.id}`
      )
      if (saved) packet = saved
    })
  }
  async function connectPayer() {
    await session.run(async () => {
      const c = await connectIdentity(payerOrigin, [config.canisters.handle, ledger])
      wallet = new WalletClient(c.api.agent, c.identity.getPrincipal(), session.crypto)
      client = new HandleClient(
        account!,
        c.api.handle!,
        config.canisters.handle,
        c.identity.getPrincipal()
      )
      job = await client.purchaseJob()
    })
  }
</script>

<section class="settings-section">
  <h2>购买与转移名称</h2>
  <label
    >账户登录来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button class="secondary" disabled={session.busy || !session.meta?.account} onclick={connect}
    >连接并核对名称服务</button
  >
  <label>名称<input bind:value={name} maxlength="20" /></label>
  <details>
    <summary>购买新名称</summary>
    <label
      >付款身份来源<select bind:value={payerOrigin}
        >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
      ></label
    >
    <button class="secondary" disabled={!account || session.busy} onclick={connectPayer}
      >连接 PANDA 付款身份</button
    >
    {#if wallet}<p>
        付款人：<code>{wallet.owner.toText()}</code> · 账本：<code>{ledger}</code>
      </p>{/if}
    <button
      class="secondary"
      disabled={!wallet || session.busy || !name}
      onclick={() =>
        session.run(async () => {
          job = await client!.preparePurchase(name)
        })}>核对名称和固定收费</button
    >
    {#if job}<p>
        名称：{decodeHandle('register_handle', job.args)[0].intent.handle} · 注册总扣款 {job.total}
        PANDA 原子单位（含注册网络费）；首次 ICRC-2 授权另收 {String(
          decodeHandle('register_handle', job.args)[0].fee
        )} 原子单位。状态：{job.phase}
      </p>
      <label
        >原扣款区块（需要对账时填写）<input bind:value={block} inputmode="numeric" /></label
      >
      <button
        class="primary"
        disabled={!wallet || session.busy}
        onclick={() =>
          session.run(async () => {
            try {
              const result = await client!.purchase(wallet!, block ? BigInt(block) : undefined)
              status = Object.keys(result.phase)[0]
            } finally {
              job = await client!.purchaseJob()
            }
          })}>批准固定额度并注册 / 对账原扣款</button
      >
    {/if}
  </details>
  <details>
    <summary>双方批准名称转移</summary>
    <p>
      只转移名称指向，不转移账户、内容或认证身份。接收方批准后，原 owner
      须在一分钟内批准并提交；过期可重新批准同一意图。
    </p>
    <label>目标账户 Xid<input bind:value={target} maxlength="20" /></label>
    <button
      class="secondary"
      disabled={!client || session.busy || !target || !name}
      onclick={() =>
        session.run(async () => {
          packet = await client!.prepareTransfer(name, target)
          review = client!.inspectTransfer(packet)
        })}>生成精确转移提案</button
    >
    <label
      >转移提案<textarea bind:value={packet} oninput={() => (review = null)} rows="4"
      ></textarea></label
    >
    <button
      class="secondary"
      disabled={!client || session.busy || !packet}
      onclick={() =>
        session.run(async () => {
          review = client!.inspectTransfer(packet)
        })}>解析并核对提案</button
    >
    {#if review}<p>
        {review.from.handle} · 第 {String(review.from.expected_version)} 版 · {xidText(
          Uint8Array.from(review.from.account_id)
        )} → {xidText(Uint8Array.from(review.accept.account_id))}
      </p>
      <button
        class="secondary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            await client!.transfer(packet, false)
            status = '目标账户已批准，请由原 owner 提交同一提案。'
          })}>作为目标批准接收</button
      >
      <button
        class="primary"
        disabled={session.busy}
        onclick={() =>
          session.run(async () => {
            await client!.transfer(packet, true)
            status = '名称转移已提交。'
          })}>作为原 owner 批准并提交</button
      >
    {/if}
  </details>
  {#if status}<p role="status">{status}</p>{/if}
</section>
