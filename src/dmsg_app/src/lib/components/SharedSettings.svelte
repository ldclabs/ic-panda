<script lang="ts">
  import { session, downloadBlob } from '../session.svelte'
  import { config } from '../config'
  import { login, services } from '../services/ic'
  import { AccountClient } from '../services/account'
  import { CloudClient } from '../services/relay'
  import {
    SharedMigrationClient,
    type MigrationDraft,
    type SharedView,
    type LegacyApproval,
    type SharedSourceFile
  } from '../services/shared-migration'
  import { id } from '../protocol/codec'
  let grantRecovery = $state('')
  let archives = $state<{ key: string; principal: string; objects: number }[]>([]),
    archiveKey = $state(''),
    grantMember = $state(''),
    grantId = $state(id())
  let client = $state.raw<SharedMigrationClient | null>(null)
  let origin = $state(config.derivationOrigins[0]),
    sourceText = $state(''),
    draftText = $state(''),
    approvalText = $state(''),
    challengeText = $state('')
  let manager = $state(''),
    member = $state(''),
    version = $state(1),
    sourceKey = $state(''),
    status = $state('')
  let draft = $state<MigrationDraft | null>(null),
    view = $state<SharedView | null>(null),
    challenge = $state<any>(null),
    invitation = $state('')
  async function connect() {
    await session.run(async () => {
      if (!session.meta?.account || !config.relayOrigin || !config.legacy.cutover)
        throw new Error(
          '先配置已审核的冻结 cutover，并连接正式工作区。个人档案迁移可独立继续。'
        )
      const identity = await login(session.crypto, session.meta.transportPublic, origin),
        api = await services(identity)
      const account = new AccountClient(
        api.user!,
        api.agent,
        identity.getPrincipal(),
        session.crypto,
        session.meta,
        config.canisters.user
      )
      if ((await account.connectedAccount()) !== session.meta.account.id)
        throw new Error('登录账户与当前工作区不一致。')
      client = new SharedMigrationClient(
        account,
        new CloudClient({ origin: config.relayOrigin, environment: config.environment }),
        session.meta.account.id,
        config.legacy
      )
      archives = await session.crypto.call('legacyList')
      status = '已连接，继承方案仍需逐一核对管理员同意。'
    })
  }
  function exportValue(value: unknown, name: string) {
    downloadBlob(
      new Blob([JSON.stringify(value, null, 2)], { type: 'application/json' }),
      name
    )
  }
  async function prepare() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      draft = await client.prepare(JSON.parse(sourceText) as SharedSourceFile, version)
      draftText = JSON.stringify(draft, null, 2)
      status = '方案已保存，请让全部冻结 managers 对此版本逐一批准。'
    })
  }
  async function loadDraft() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      const value = JSON.parse(draftText) as MigrationDraft
      await client.validateDraft(value)
      draft = value
      status = '来源及新 genesis 签名已核对。'
    })
  }
  async function managerRequest() {
    await session.run(async () => {
      if (!client || !draft) throw new Error('先核对方案。')
      challenge = await client.managerChallenge(draft, manager)
      challengeText = JSON.stringify(challenge, null, 2)
    })
  }
  async function vote(proposing: boolean) {
    await session.run(async () => {
      if (!client || !draft) throw new Error('先核对方案。')
      const approval = JSON.parse(approvalText) as LegacyApproval
      view = proposing
        ? await client.propose(draft, manager, approval)
        : await client.consent(draft, manager, approval)
      sourceKey = view.key
      status = '批准已记入该版本，未齐全时不能提交官方继承。'
    })
  }
  async function refresh() {
    await session.run(async () => {
      if (!client) throw new Error('请先连接账户。')
      view = await client.view(sourceKey)
    })
  }
  async function commit(activate: boolean) {
    await session.run(async () => {
      if (!client || !view) throw new Error('先读取目录状态。')
      view = activate ? await client.activate(view) : await client.commit(view)
      status = activate
        ? '官方继承已建立并完成初始换代。尚未认领的旧成员不会自动加入。'
        : '唯一继承映射已提交，下一步创建指定频道。'
    })
  }
  async function claimRequest() {
    await session.run(async () => {
      if (!client || !view) throw new Error('先载入完整目录记录。')
      challenge = await client.claimChallenge(view, member)
      challengeText = JSON.stringify(challenge, null, 2)
    })
  }
  async function claim() {
    await session.run(async () => {
      if (!client || !view || !challenge?.claim) throw new Error('先核对成员认领请求。')
      view = await client.claim(view, challenge.claim, JSON.parse(approvalText))
      status = '旧成员认领已核对，仍需接受新频道邀请并等待新 epoch。'
    })
  }
</script>

<section class="settings-section">
  <h2>共享旧频道的官方继承</h2>
  <p>
    来源使用旧 canister 与频道 ID 的完整组合。每个冻结 manager 必须同意同一新 owner 与
    genesis；未达成共识时，可以继续个人档案迁移。
  </p>
  <label
    >账户登录来源<select bind:value={origin}
      >{#each config.derivationOrigins as value}<option>{value}</option>{/each}</select
    ></label
  ><button class="secondary" disabled={session.busy} onclick={connect}
    >连接并核对冻结配置</button
  >
  <details>
    <summary>提出继承方案</summary><label
      >原 origin 导出的冻结权限证明<textarea bind:value={sourceText} rows="4"
      ></textarea></label
    ><label
      >方案版本（首次为 1）<input
        type="number"
        min="1"
        max="1000"
        bind:value={version}
      /></label
    ><button class="secondary" disabled={!client || session.busy} onclick={prepare}
      >验证来源并生成方案</button
    >
  </details>
  <label>完整继承方案<textarea bind:value={draftText} rows="5"></textarea></label><button
    class="secondary"
    disabled={!client || session.busy || !draftText}
    onclick={loadDraft}>核对方案</button
  >
  {#if draft}<dl class="evidence-list">
      <div>
        <dt>旧来源</dt>
        <dd>{draft.proposal.source} / {draft.proposal.channel}</dd>
      </div>
      <div>
        <dt>新 owner</dt>
        <dd>{draft.proposal.owner}</dd>
      </div>
      <div>
        <dt>新频道</dt>
        <dd>{draft.proposal.channel_id}</dd>
      </div>
      <div>
        <dt>方案版本 / genesis</dt>
        <dd>{draft.proposal.version} / {draft.proposal.genesis_digest}</dd>
      </div>
    </dl>
    <button
      class="secondary"
      onclick={() => exportValue(draft, 'shared-migration-proposal.json')}
      >导出此版本方案</button
    >{/if}
  <label>本次代表的旧 manager Principal<input bind:value={manager} /></label><button
    class="secondary"
    disabled={!draft || !client || session.busy || !manager}
    onclick={managerRequest}>生成管理员批准请求</button
  >
  {#if challengeText}<label
      >在原 origin 核对并批准<textarea readonly value={challengeText} rows="5"
      ></textarea></label
    >{/if}
  <label
    >原 origin 返回的批准结果<textarea bind:value={approvalText} rows="4"></textarea></label
  >
  <button
    class="primary"
    disabled={!client || !draft || session.busy || !approvalText}
    onclick={() => vote(true)}>提交我的新 owner 方案及批准</button
  ><button
    class="secondary"
    disabled={!client || !draft || session.busy || !approvalText}
    onclick={() => vote(false)}>为已有方案记录我的批准</button
  >
</section>
<section class="settings-section">
  <h3>唯一继承目录与成员认领</h3>
  <label>来源目录键<input bind:value={sourceKey} maxlength="64" /></label><button
    class="secondary"
    disabled={!client || session.busy || !sourceKey}
    onclick={refresh}>读取 / 对账原状态</button
  >
  <label
    >或载入受信参与者发来的完整目录记录<textarea
      rows="4"
      onchange={(event) => {
        try {
          const value = JSON.parse(event.currentTarget.value) as SharedView
          void session.run(async () => {
            if (!client) throw new Error('请先连接账户。')
            view = await client.validateView(value)
            sourceKey = view.key
          })
        } catch {
          session.error = '目录记录不是有效 JSON。'
        }
      }}></textarea></label
  >
  {#if view}<p>
      状态：{view.stage} · 版本 {view.proposal.version} · {Object.keys(view.votes).length} / {view
        .source.managers.length} 个管理员同意。
    </p>
    <ul>
      {#each view.source.managers as value}<li>
          {value}：{view.votes[value] ? '已批准此版本' : '尚未批准'}
        </li>{/each}
    </ul>
    <button
      class="secondary"
      onclick={() => exportValue(view, 'shared-migration-directory.json')}
      >导出目录证明供其他成员核对</button
    >
    {#if view.proposal.owner === session.meta?.account?.id}<button
        class="primary"
        disabled={!client || session.busy || view.source.managers.some((m) => !view!.votes[m])}
        onclick={() => commit(false)}>提交全部同意的官方映射</button
      ><button
        class="secondary"
        disabled={!client || session.busy || !['committed', 'active'].includes(view.stage)}
        onclick={() => commit(true)}>创建 / 对账指定继承频道</button
      >{/if}
    <label>要认领的旧成员 Principal<input bind:value={member} /></label><button
      class="secondary"
      disabled={!client || session.busy || !member}
      onclick={claimRequest}>生成绑定本账户设备的认领请求</button
    ><button
      class="primary"
      disabled={!client || session.busy || !challenge?.claim || !approvalText}
      onclick={claim}>提交旧身份及新设备的明确认领</button
    >
    {#each Object.values(view.claims) as value}<div class="settings-row">
        <span
          >{value.claim.member} → {value.claim.account} · {value.joined
            ? '已接受并进入新 epoch'
            : '尚待新频道接受'}</span
        >{#if view.proposal.owner === session.meta?.account?.id}<button
            class="secondary"
            disabled={!client || session.busy}
            onclick={() =>
              session.run(async () => {
                invitation = JSON.stringify(
                  await client!.channels.invite(
                    view!.proposal.channel_id,
                    value.claim.account
                  ),
                  null,
                  2
                )
              })}>为已认领成员生成邀请</button
          >{/if}
      </div>{/each}
    {#if invitation}<label
        >发给已认领账户的邀请<textarea readonly value={invitation} rows="4"></textarea></label
      >{/if}
    <button
      class="secondary"
      disabled={!client || session.busy}
      onclick={() =>
        session.run(async () => {
          view = await client!.joined(view!)
        })}>确认本人已接受且新 epoch 已生效</button
    >
    <h3>单独交付旧频道历史</h3>
    <p>
      仅封装本来源频道的历史消息、附件和频道钥。接收范围绑定已认领账户的指定新设备及恢复公钥。
    </p>
    <label
      >本机已验证的个人迁移档案<select bind:value={archiveKey}
        ><option value="">请选择</option>{#each archives as archive}<option value={archive.key}
            >{archive.principal} · {archive.objects} 项</option
          >{/each}</select
      ></label
    >
    <label
      >已认领且已加入的旧成员<select bind:value={grantMember}
        ><option value="">请选择</option
        >{#each Object.values(view.claims).filter((c) => c.joined) as value}<option
            value={value.claim.member}>{value.claim.member} → {value.claim.account}</option
          >{/each}</select
      ></label
    >
    <button
      class="primary"
      disabled={!client || session.busy || !archiveKey || !grantMember}
      onclick={() =>
        session.run(async () => {
          view = await client!.shareHistory(view!, archiveKey, grantMember, grantId)
          grantId = id()
          status = '定向历史授权已保存。接收者需独立校验与恢复。'
        })}>批准并封装此频道历史</button
    >
    <label
      >若原接收设备已丢失，可输入该信封所属代的恢复码<input
        type="password"
        bind:value={grantRecovery}
        autocomplete="off"
      /></label
    >
    {#each Object.values(view.grants ?? {}).filter((g) => g.grant.scope.account === session.meta?.account?.id) as value}<div
        class="settings-row"
      >
        <span>旧频道历史 · {value.grant.scope.archive_digest}</span><button
          class="secondary"
          disabled={!client || session.busy}
          onclick={() =>
            session.run(async () => {
              const result = await client!.receiveHistory(
                view!,
                value.grant.scope.grant_id,
                grantRecovery || undefined
              )
              grantRecovery = ''
              status = `已导入 ${result.count} 项；${result.gaps.length} 项缺口。请在旧版迁移页检查并导出恢复包。`
              await session.refresh()
            })}>接收、解密并验证</button
        >
      </div>{/each}
  {/if}
  {#if status}<p role="status">{status}</p>{/if}
</section>
