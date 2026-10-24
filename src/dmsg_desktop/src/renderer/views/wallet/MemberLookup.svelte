<script lang="ts">
  import { onMount } from 'svelte'
  import { accountOf, nameOf } from '../channels/names.svelte'
  import Field from '../../components/page/Field.svelte'
  import { buttonClass } from '../../components/ui'
  import { isMemberId } from './new-wallet'
  import { walletError } from './errors'
  import { w } from './text'
  let {
    id = $bindable(''),
    label = $bindable(''),
    disabled = false,
    testid
  }: { id: string; label?: string; disabled?: boolean; testid?: string } = $props()
  let busy = $state(false),
    error = $state(''),
    alive = true
  onMount(() => () => {
    alive = false
  })
  async function resolve() {
    const asked = id
    busy = true
    error = ''
    try {
      const account = await accountOf(asked)
      if (alive && id === asked) {
        id = account
        if (!label) label = asked.trim().replace(/^@/, '')
      }
    } catch (e) {
      if (alive) error = walletError(e)
    } finally {
      if (alive) busy = false
    }
  }
</script>

<div class="grid gap-2">
  <Field label={w('memberLookup')}
    ><input
      bind:value={id}
      {disabled}
      spellcheck="false"
      autocomplete="off"
      dir="ltr"
      data-testid={testid}
      aria-invalid={!!id && !isMemberId(id) && !!error}
    /></Field
  >
  {#if id && !isMemberId(id)}<button
      class={buttonClass('secondary', 'sm', 'justify-self-start')}
      disabled={disabled || busy}
      onclick={resolve}>{w(busy ? 'loading' : 'resolveMember')}</button
    >{/if}
  {#if isMemberId(id)}<p class="text-[12px] break-all">{id.length === 20 ? (nameOf(id) ?? '') : ''} · {id}</p><p
      class="text-muted text-[12px]">{w('identityReview')}</p
    >{/if}
  {#if error}<p class="text-danger text-[12px]" role="alert">{error}</p>{/if}
</div>
