<script lang="ts">
  import IconCircleSpin from '$lib/components/icons/IconCircleSpin.svelte'
  import ModalCard from '$lib/components/ui/ModalCard.svelte'
  import { toastRun } from '$lib/stores/toast'
  import { MasterKey, type MyMessageState } from '$src/lib/stores/message'
  import {
    focusTrap,
    getModalStore,
    getToastStore
  } from '@skeletonlabs/skeleton'
  import { onMount, type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let myState: MyMessageState
  export let masterKey: MasterKey | null

  const toastStore = getToastStore()
  const modalStore = getModalStore()
  const PasswordExpire = 14 * 24 * 60 * 60 * 1000

  let validating = false
  let submitting = false

  let isSetup = !masterKey
  let passwordInput1 = ''
  let passwordInput2 = ''
  let passwordTip = ''
  let processingTip = 'Derive keys...'
  let cachedPassword = masterKey ? masterKey.cachedPassword() : true
  let pwdHash: Uint8Array | null = null

  function checkPassword() {
    passwordTip = ''
    if (passwordInput1.length < 6) {
      passwordTip = 'Password must be at least 6 characters long'
    } else if (passwordInput1 !== passwordInput2) {
      passwordTip = 'Passwords do not match'
    } else if (passwordInput2.length > 10) {
      passwordTip = 'Recommended to use a easy-to-remember password'
    }
    return ''
  }

  function onComfirm() {
    submitting = true

    toastRun(async () => {
      const kind = myState.masterKeyKind()
      const myIV = await myState.myIV()
      // master key has been derived
      if (masterKey || pwdHash) {
        try {
          if (masterKey) {
            await masterKey.open(
              passwordInput1,
              myState.id,
              cachedPassword ? PasswordExpire : 0,
              myIV
            )
            pwdHash = masterKey.passwordHash()
            await myState.savePasswordHash(pwdHash)
          } else {
            const remoteKey = await myState.fetchECDHCoseEncryptedKey()
            const remoteSecret = remoteKey.getSecretKey()
            masterKey = await myState.setMasterKey(
              kind,
              passwordInput1,
              remoteSecret,
              cachedPassword ? PasswordExpire : 0,
              myIV
            )
          }

          if (masterKey.kind !== kind) {
            processingTip = 'Migrate encrypted keys to ICP chain ...'
            await myState.migrateKeys(myIV)
          } else {
            await myState.initStaticECDHKey()
          }

          modalStore.close()
        } catch (err) {
          const er = new Error('Incorrect password')
          ;(er as any).data = err
          throw er
        }

        return
      }

      if (passwordInput1 != passwordInput2) {
        throw new Error('Passwords do not match')
      }

      let remoteSecret: Uint8Array
      switch (kind) {
        case 'Local':
          remoteSecret = new Uint8Array(myIV)
          break
        case 'ECDH':
          const remoteKey = await myState.fetchECDHCoseEncryptedKey()
          remoteSecret = remoteKey.getSecretKey()
          break
        default:
          throw new Error('Invalid master key kind')
      }

      masterKey = await myState.setMasterKey(
        kind,
        passwordInput1,
        remoteSecret,
        cachedPassword ? PasswordExpire : 0,
        myIV
      )
      pwdHash = masterKey.passwordHash()
      await myState.savePasswordHash(pwdHash)
      await myState.initStaticECDHKey()

      modalStore.close()
    }, toastStore).finally(() => {
      submitting = false
      validating = false
    })
  }

  function onFormChange(e: Event) {
    e.stopPropagation()
    e.preventDefault()

    const form = e.currentTarget as HTMLFormElement
    ;(form['passwordInput2'] as HTMLInputElement)?.setCustomValidity(
      checkPassword()
    )
    validating = form.checkValidity()
  }

  onMount(() => {
    const { abort } = toastRun(async () => {
      pwdHash = await myState.getPasswordHash()
      if (pwdHash) {
        isSetup = false
        passwordTip = 'Derive master key with remote key material'
      }
    }, toastStore)
    return abort
  })
</script>

<ModalCard {parent}>
  <div class="!mt-0 text-center text-xl font-bold"
    >{isSetup ? 'Setup' : 'Enter'} Password</div
  >
  <form
    class="m-auto !mt-4 flex flex-col content-center"
    on:input|preventDefault|stopPropagation={onFormChange}
    use:focusTrap={true}
  >
    <input
      class="input truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
      type="password"
      name="passwordInput1"
      minlength="6"
      maxlength="32"
      bind:value={passwordInput1}
      disabled={submitting}
      placeholder="Enter password"
      data-focusindex="0"
      required
    />
    {#if isSetup}
      <input
        class="input mt-4 truncate rounded-xl border-gray/10 bg-white/20 invalid:input-warning hover:bg-white/90"
        type="password"
        name="passwordInput2"
        minlength="6"
        maxlength="32"
        bind:value={passwordInput2}
        disabled={submitting}
        placeholder="Enter password again"
        data-focusindex="1"
        required
      />
    {/if}
    <p
      class="h-5 pl-2 text-sm text-error-500 {passwordTip == ''
        ? 'invisible'
        : 'visiable'}">{passwordTip}</p
    >
    {#if isSetup}
      <hr class="!border-t-1 mx-[-24px] !border-dashed !border-gray/20" />
      <div class="!mt-4 space-y-2 rounded-xl bg-gray/5 p-4">
        <p class="">
          <b>1.</b> The password is used only locally to derive the master key with
          remote key material; neither the password nor the derived key is stored
          remotely.
        </p>
        <p class="">
          <b>2.</b> It is recommended to use
          <b>simple and easy-to-remember passwords</b>.
        </p>
        <p class="text-error-500">
          <b>3.</b> If you forget the password, you will no longer be able to decrypt
          the messages!
        </p>
      </div>
    {/if}
    <label class="mt-2 flex items-center">
      <input class="checkbox" type="checkbox" bind:checked={cachedPassword} />
      <p class="ml-2 text-sm text-gray/50">Retain for 14 days</p>
    </label>
  </form>
  <footer class="m-auto !mt-4">
    <button
      class="variant-filled-primary btn w-full text-white"
      disabled={submitting || !validating}
      on:click={onComfirm}
    >
      {#if submitting}
        <span class=""><IconCircleSpin /></span>
        <span>{processingTip}</span>
      {:else}
        <span>Comfirm</span>
      {/if}
    </button>
  </footer>
</ModalCard>
