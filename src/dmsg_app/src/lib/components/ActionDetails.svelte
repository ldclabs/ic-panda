<script lang="ts">
  import { Principal } from '@icp-sdk/core/principal'
  import {
    actionCommandSchema,
    type ActionLabel,
    type ActionValue,
    type AppAction,
    type AppRegistration,
    type FieldSchema,
    type FieldType
  } from 'dmsg-sdk'
  import { hex } from '../protocol/codec'
  import { dateLabel } from '../session.svelte'
  // `app` is the certified registration the action was admitted against; its
  // schema is the only source of titles and labels.
  let { action, app }: { action: AppAction; app: AppRegistration } = $props()
  const command = $derived(actionCommandSchema(action, app.action_schema!))
  const text = (labels: ActionLabel[]) =>
    (
      labels.find((l) => l.locale.startsWith('zh')) ??
      labels.find((l) => l.locale.startsWith('en')) ??
      labels[0]!
    ).text
  const kind = (ty: FieldType) => (typeof ty === 'string' ? ty : Object.keys(ty)[0]!)
  const inner = (ty: FieldType): any => (typeof ty === 'string' ? null : Object.values(ty)[0])
  const raw = (v: ActionValue): any => (v === 'Null' ? null : Object.values(v)[0])
</script>

{#snippet value(ty: FieldType, v: ActionValue)}
  {@const t = inner(ty)}
  {@const x = raw(v)}
  {#if v === 'Null'}未提供
  {:else if kind(ty) === 'Optional'}{@render value(t.item, v)}
  {:else if kind(ty) === 'Nat'}{x.toString()}
  {:else if kind(ty) === 'Bool'}{text(x ? t.yes : t.no)}
  {:else if kind(ty) === 'Text'}<span class:preserve-lines={t.multiline}>{x}</span>
  {:else if kind(ty) === 'Hash'}<code class="hash">{hex(x)}</code>
  {:else if kind(ty) === 'Principal'}<code>{Principal.fromUint8Array(x).toText()}</code>
  {:else if kind(ty) === 'Choice'}{text(
      t.options.find((o: { value: string }) => o.value === x).label
    )}
  {:else if kind(ty) === 'Artifact'}{x.uri}<br />{x.content_type} · {x.size.toString()} bytes<br
    /><code class="hash">{hex(x.sha256)}</code>
  {:else if kind(ty) === 'List'}{#if x.length === 0}无{/if}{#each x as item}<div>
        {@render value(t.item, item)}
      </div>{/each}
  {:else if kind(ty) === 'Record'}{@render fields(t.fields, x)}
  {/if}
{/snippet}

{#snippet fields(schema: FieldSchema[], args: AppAction['command']['args'])}
  <dl class="evidence-list">
    {#each schema as field, i}<div>
        <dt>{text(field.label)}</dt>
        <dd>{@render value(field.ty, args[i]!.value)}</dd>
      </div>{/each}
  </dl>
{/snippet}

<h2>{text(command.title)}</h2>
<dl class="evidence-list">
  <div>
    <dt>应用</dt>
    <dd>{action.app_id}</dd>
  </div>
  <div>
    <dt>接收 canister</dt>
    <dd><code>{Principal.fromUint8Array(action.receiver).toText()}</code></dd>
  </div>
  <div>
    <dt>产品账户</dt>
    <dd><code>{hex(action.actor)}</code></dd>
  </div>
  <div>
    <dt>有效期</dt>
    <dd>
      {dateLabel(Number(action.issued_at_ms))} — {dateLabel(Number(action.expires_at_ms))}
    </dd>
  </div>
  <div>
    <dt>动作定义</dt>
    <dd>
      应用登记第 {action.app_config_version.toString()} 版 · {action.command.name}
      <code class="hash">{hex(action.schema_hash)}</code>
    </dd>
  </div>
</dl>
{@render fields(command.fields, action.command.args)}
{#each action.files as file}<section class="review-content">
    <strong>{file.display_name ?? file.file_id}</strong>
    <p>
      版本 {file.revision.toString()} · {file.media_type} · {file.byte_length.toString()} bytes ·
      {file.representation === 'Original' ? '原文件' : '密文'}
    </p>
    <code class="hash">{hex(file.sha256)}</code>
  </section>{/each}
<p class="notice warning">
  动作名称和字段说明来自该应用经治理登记的定义，dMsg 保证这里显示的就是签名内容。此处核对的是动作和文件承诺，未取得原文件。项目会在提交时再次检查权限、版本和期限；完成签署不等于已提交成功。
</p>
