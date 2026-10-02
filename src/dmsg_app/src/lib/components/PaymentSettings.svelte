<script lang="ts">
  import { session } from '../session.svelte'
  import { config } from '../config'
  import { connectIdentity } from '../connection'
  import { PaymentClient } from '../services/payment'
  import { hex } from '../protocol/codec'
  import type { EscrowInfo } from '../canisters/generated/payment'
  import PaymentRecovery from './PaymentRecovery.svelte'
  let client = $state.raw<PaymentClient | null>(null)
  let origin = $state(config.derivationOrigins[0]),
    owner = $state(''),
    id = $state(''),
    block = $state('')
  let rows = $state<EscrowInfo[]>([]),
    escrow = $state<EscrowInfo | null>(null)
  async function load(after?: string) {
    rows = await client!.list(after)
  }
  async function connect() {
    await session.run(async () => {
      const connection = await connectIdentity(origin, [config.canisters.payment])
      if (!connection.api.payment) throw new Error('尚未配置支付服务。')
      owner = connection.identity.getPrincipal().toText()
      client = new PaymentClient(
        connection.api.payment,
        connection.api.agent,
        config.canisters.payment
      )
      await load()
    })
  }
</script>

<section class="settings-section">
  <h2>直接恢复托管资金</h2>
  <p>使用原付款身份直接查询支付服务，无需云端中继、原设备记录或正式内容根。</p>
  <label
    >付款身份来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  >
  <button
    class="secondary"
    disabled={session.busy || !config.canisters.payment}
    onclick={connect}>连接并读取我的托管单</button
  >
  {#if owner}<p><code>{owner}</code></p>{/if}
  {#each rows as row}<button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          id = hex(Uint8Array.from(row.escrow_id))
          escrow = await client!.escrow(id)
        })}>{hex(Uint8Array.from(row.escrow_id))} · {Object.keys(row.decision)[0]}</button
    >{/each}
  {#if rows.length}<button
      class="secondary"
      disabled={session.busy}
      onclick={() => session.run(() => load(hex(Uint8Array.from(rows.at(-1)!.escrow_id))))}
      >下一页托管单</button
    >{/if}
  <label>托管 ID<input bind:value={id} maxlength="64" /></label>
  <button
    class="secondary"
    disabled={!client || session.busy || !/^[0-9a-f]{64}$/.test(id)}
    onclick={() =>
      session.run(async () => {
        escrow = await client!.escrow(id)
      })}>认证读取指定托管</button
  >
  {#if client && escrow}
    <p>
      资金决定：{Object.keys(escrow.decision)[0]} · 已确认入账 {String(escrow.confirmed_in)}
    </p>
    <label>原入金区块<input bind:value={block} inputmode="numeric" /></label>
    <button
      class="secondary"
      disabled={session.busy || !/^[0-9]+$/.test(block)}
      onclick={() =>
        session.run(async () => {
          escrow = await client!.funding(
            hex(Uint8Array.from(escrow!.escrow_id)),
            BigInt(block)
          )
        })}>核对原入金</button
    >
    <button
      class="secondary"
      disabled={session.busy}
      onclick={() =>
        session.run(async () => {
          escrow = await client!.refund(hex(Uint8Array.from(escrow!.escrow_id)))
        })}>核对到期并申请退款</button
    >
    <PaymentRecovery {client} {escrow} />
  {/if}
</section>
