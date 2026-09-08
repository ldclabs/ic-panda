<script lang="ts">
  import LocaleSwitcher from '$lib/i18n/LocaleSwitcher.svelte'
  import { t } from '$lib/i18n'
  import { goto } from '$app/navigation'
  import SignInModal from '$lib/components/core/SignInModal.svelte'
  import MoreMenuPopup from '$lib/components/core/MoreMenuPopup.svelte'
  import Brand from '$lib/components/landing/Brand.svelte'
  import Icon from '$lib/components/landing/Icon.svelte'
  import { MyMessageState } from '$lib/stores/message'
  import { authStore } from '$lib/stores/auth'
  import { getModalStore } from '$lib/ui/stores'
  import '$lib/components/landing/landing.css'

  const modalStore = getModalStore()
  let loading = $state(false)
  let error = $state('')
  let noAccount = $state(false)

  async function openArchive() {
    if ($authStore.identity.getPrincipal().isAnonymous()) {
      modalStore.trigger({
        type: 'component',
        title: $t('Sign in to your archive'),
        component: { ref: SignInModal, props: { onCompleted: openArchive } }
      })
      return
    }
    loading = true
    error = ''
    noAccount = false
    try {
      const state = await MyMessageState.load()
      if (!state.api.myInfo) {
        noAccount = true
      } else {
        await goto('/_/messages')
      }
    } catch (cause) {
      error =
        cause instanceof Error
          ? cause.message
          : 'Could not load your account. Please try again.'
    } finally {
      loading = false
    }
  }
</script>

<svelte:head
  ><title>{$t('dMsg — Legacy archive')}</title><meta
    name="robots"
    content="noindex"
  /></svelte:head
>
<div class="dmsg-site legacy-entry">
  <header class="site-header"
    ><div class="wrap nav-wrap"
      ><Brand />
      <LocaleSwitcher /><a class="text-link" href="/" data-sveltekit-reload
        >{$t('Back to dMsg')}<Icon name="arrow-right" /></a
      ></div
    ></header
  >
  <main class="wrap">
    <section class="section legacy-intro">
      <div class="eyebrow section-label">{$t('Legacy app / Read-only')}</div>
      <h1>{$t('Your history.')}<br />{$t('Still in your hands.')}</h1>
      <p class="lead"
        >{$t(
          'Read your existing conversations and download the attachments you can already access.'
        )}</p
      >
      <p
        >{$t(
          'New messages, uploads, profile edits, username changes, and payments are disabled in this app. The new Chrome workspace and migration flow are not available yet.'
        )}</p
      >
      <div class="hero-actions"
        ><button class="button primary" onclick={openArchive} disabled={loading}
          >{loading
            ? $t('Opening archive…')
            : $authStore.identity.getPrincipal().isAnonymous()
              ? $t('Sign in to read your history')
              : $t('Open message archive')}<Icon name="arrow-right" /></button
        >{#if !$authStore.identity.getPrincipal().isAnonymous()}<MoreMenuPopup
            triggerClass="text-link"
            >{#snippet trigger()}{$t(
                'Account options'
              )}{/snippet}</MoreMenuPopup
          >{/if}</div
      >
      {#if error}<p class="entry-error" role="alert">{$t(error)}</p><button
          class="text-link"
          onclick={openArchive}>{$t('Try again')}</button
        >{/if}
      {#if noAccount}<div class="entry-error" role="status"
          ><strong>{$t('No legacy account found for this identity.')}</strong><p
            >{$t(
              'Use the original sign-in provider or switch to your existing username account in Account options. New account registration is closed.'
            )}</p
          ><button class="text-link" onclick={() => authStore.logout('/legacy')}
            >{$t('Sign out and use another identity')}</button
          ></div
        >{/if}
    </section>
    <section class="legacy-guidance rule"
      ><div
        ><span class="step-number">{$t('01 / KEEP YOUR BROWSER DATA')}</span><h2
          >{$t('Start where your keys live.')}</h2
        ><p
          >{$t(
            'If you used Local key mode, use the original browser and this original website address. Do not clear site data before preserving your recovery materials.'
          )}</p
        ></div
      ><div
        ><span class="step-number">{$t('02 / USE YOUR EXISTING IDENTITY')}</span
        ><h2>{$t('Sign in as before.')}</h2><p
          >{$t(
            'The original Internet Identity and username account options are retained. A different login origin can lead to a different account.'
          )}</p
        ></div
      ><div
        ><span class="step-number">{$t('03 / READ AND DOWNLOAD')}</span><h2
          >{$t('Keep accessible files.')}</h2
        ><p
          >{$t(
            'Open old channels and download their attachments. Missing keys or inaccessible history are reported; no keys are reset or replaced to make an archive appear complete.'
          )}</p
        ></div
      ></section
    >
  </main>
</div>

<style>
  .legacy-intro {
    max-width: 780px;
  }
  .legacy-intro h1 {
    font-size: clamp(44px, 7vw, 80px);
    font-weight: 650;
    margin-bottom: 32px;
  }
  .legacy-intro .lead {
    font-size: 21px;
    max-width: 580px;
  }
  .legacy-intro > p {
    max-width: 620px;
  }
  .hero-actions {
    margin-top: 32px;
  }
  .legacy-guidance {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 40px;
    padding: 40px 0 80px;
  }
  .legacy-guidance h2 {
    font-size: 22px;
    margin-bottom: 16px;
  }
  .legacy-guidance p {
    font-size: 14px;
  }
  .entry-error {
    margin-top: 24px !important;
    padding: 20px;
    background: #edf1ea;
    border-radius: 8px;
  }
  .entry-error p {
    margin-top: 8px;
  }
  .legacy-entry button:disabled {
    opacity: 0.65;
    cursor: wait;
  }
  @media (max-width: 760px) {
    .legacy-guidance {
      grid-template-columns: 1fr;
      gap: 24px;
    }
    .legacy-intro .lead {
      font-size: 19px;
    }
  }
  @media (max-width: 600px) {
    .legacy-entry .site-header {
      height: auto;
    }
    .legacy-entry .nav-wrap {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 8px;
      padding-block: 12px;
    }
    .nav-wrap > .text-link {
      grid-column: 1 / -1;
      justify-self: start;
    }
  }
</style>
