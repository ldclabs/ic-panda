<script lang="ts">
  import { t } from '$lib/i18n'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { authStore } from '$lib/stores/auth'
  import type { SvelteComponent } from 'svelte'

  let {
    parent,
    onCompleted
  }: { parent: SvelteComponent; onCompleted: () => Promise<void> } = $props()
  let submitting = $state(false)
  let error = $state('')

  async function signIn(provider: 'current' | 'internetcomputer' | 'legacy') {
    if (submitting) return
    submitting = true
    error = ''
    try {
      if (provider === 'legacy') await authStore.signIn()
      else if (provider === 'current')
        await authStore.signIn2('https://id.ai/authorize')
      else await authStore.signIn2()
      parent['onClose']()
      await onCompleted()
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Sign-in was not completed. You can try again.'
    } finally {
      submitting = false
    }
  }
</script>

<ModalCard
  {parent}
  showTitle={false}
  cardClass="!rounded-2xl !bg-white !text-[#10251f]"
>
  <h2 class="!mt-0 pr-8 text-xl font-semibold"
    >{$t('Sign in to your archive')}</h2
  >
  <p class="text-sm text-[#4e6257]"
    >{$t(
      'Choose the provider you used before. Your existing account and login origin determine which history you can read.'
    )}</p
  >
  <div class="sign-in-options">
    <button disabled={submitting} onclick={() => signIn('current')}
      ><strong>Internet Identity</strong><span
        >{$t('id.ai · Current site identity')}</span
      ></button
    >
    <button disabled={submitting} onclick={() => signIn('internetcomputer')}
      ><strong>Internet Identity</strong><span
        >{$t('identity.internetcomputer.org · Current site identity')}</span
      ></button
    >
    <button disabled={submitting} onclick={() => signIn('legacy')}
      ><strong>{$t('Original legacy sign-in')}</strong><span
        >{$t('identity.ic0.app · panda.fans identity')}</span
      ></button
    >
  </div>
  {#if submitting}<p class="text-sm" role="status"
      >{$t('Waiting for sign-in…')}</p
    >{/if}
  {#if error}<p class="text-sm text-red-800" role="alert">{$t(error)}</p>{/if}
</ModalCard>

<style>
  .sign-in-options {
    display: grid;
    gap: 12px;
  }
  .sign-in-options button {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
    text-align: left;
    width: 100%;
    padding: 16px;
    border: 1px solid #607566;
    border-radius: 8px;
    min-height: 48px;
    overflow-wrap: anywhere;
  }
  .sign-in-options button:hover {
    background: #e8f0e9;
  }
  .sign-in-options button:focus-visible {
    outline: 3px solid #145c45;
    outline-offset: 3px;
  }
  .sign-in-options strong {
    font-size: 15px;
    font-weight: 600;
  }
  .sign-in-options span {
    font-size: 13px;
    color: #4e6257;
  }
  .sign-in-options button:disabled {
    opacity: 0.65;
    cursor: wait;
  }
</style>
