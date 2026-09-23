<script lang="ts">
  import { Principal } from '@icp-sdk/core/principal'
  import type { AppAction } from '@dmsg/sdk'
  import { hex } from '../protocol/codec'
  import { dateLabel } from '../session.svelte'
  let { action }: { action: AppAction } = $props()
  const command = $derived(action.command)
  const title = $derived(
    'TokenListCertifyDisclosure' in command
      ? '认证披露稿'
      : 'TokenListDecideReview' in command
        ? '记录审阅决定'
        : 'TokenListCertifyTransition' in command
          ? '认证过渡分析'
          : '批准或拒绝过渡'
  )
</script>

<h2>{title}</h2>
<dl class="evidence-list">
  <div>
    <dt>应用</dt>
    <dd>{action.app_id}</dd>
  </div>
  <div>
    <dt>接收项目</dt>
    <dd><code>{Principal.fromUint8Array(action.receiver).toText()}</code></dd>
  </div>
  <div>
    <dt>产品账户</dt>
    <dd><code>{hex(action.actor_id)}</code></dd>
  </div>
  <div>
    <dt>有效期</dt>
    <dd>
      {dateLabel(Number(action.issued_at_ms))} — {dateLabel(Number(action.expires_at_ms))}
    </dd>
  </div>
  {#if 'TokenListCertifyDisclosure' in command}
    <div>
      <dt>项目 / 合同 / 稿件版本</dt>
      <dd>
        {command.TokenListCertifyDisclosure.project_id.toString()} / {command.TokenListCertifyDisclosure.contract_id.toString()}
        / {command.TokenListCertifyDisclosure.revision.toString()}
      </dd>
    </div>
  {:else if 'TokenListDecideReview' in command}
    {@const review = command.TokenListDecideReview}
    <div>
      <dt>项目 / 审阅 / 轮次</dt>
      <dd>
        {review.project_id.toString()} / {review.case_id.toString()} / {review.round.toString()}
      </dd>
    </div>
    <div>
      <dt>决定</dt>
      <dd>
        {review.outcome === 'Approved'
          ? '批准'
          : review.outcome === 'Rejected'
            ? '拒绝'
            : '要求修改'}
      </dd>
    </div>
    <div>
      <dt>理由</dt>
      <dd class="preserve-lines">{review.rationale}</dd>
    </div>
    {#each review.changes as change}<div>
        <dt>{change.locator}{change.blocking ? '（必须解决）' : ''}</dt>
        <dd class="preserve-lines">{change.detail}</dd>
      </div>{/each}
  {:else}
    {@const transition =
      'TokenListCertifyTransition' in command
        ? command.TokenListCertifyTransition
        : command.TokenListApproveTransition}
    <div>
      <dt>项目 / 过渡</dt>
      <dd>{transition.project_id.toString()} / {transition.transition_id.toString()}</dd>
    </div>
    {#if 'approve' in transition}<div>
        <dt>决定</dt>
        <dd>{transition.approve ? '批准' : '拒绝'}</dd>
      </div>{/if}
    <div>
      <dt>理由</dt>
      <dd class="preserve-lines">{transition.rationale}</dd>
    </div>
    <div>
      <dt>声明摘要</dt>
      <dd><code class="hash">{hex(transition.statement_hash)}</code></dd>
    </div>
    {#if 'analysis' in transition && transition.analysis}<div>
        <dt>分析文件</dt>
        <dd>
          {transition.analysis.uri}<br />{transition.analysis.content_type} · {transition.analysis.size.toString()}
          bytes<br /><code class="hash">{hex(transition.analysis.sha256)}</code>
        </dd>
      </div>{/if}
  {/if}
</dl>
{#each action.files as file}<section class="review-content">
    <strong>{file.display_name ?? file.file_id}</strong>
    <p>
      版本 {file.revision.toString()} · {file.media_type} · {file.byte_length.toString()} bytes ·
      {file.representation === 'Original' ? '原文件' : '密文'}
    </p>
    <code class="hash">{hex(file.sha256)}</code>
  </section>{/each}
<p class="notice warning">
  此处核对的是动作和文件承诺，未取得原文件。项目会在提交时再次检查权限、版本和期限；完成签署不等于已提交成功。
</p>
