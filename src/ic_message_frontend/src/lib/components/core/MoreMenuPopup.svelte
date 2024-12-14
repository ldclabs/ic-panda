<script lang="ts">
  import WalletDetailModal from '$lib/components/core/WalletDetailModal.svelte'
  import IconExchange from '$lib/components/icons/IconExchangeDollar.svelte'
  import IconGithub from '$lib/components/icons/IconGithub.svelte'
  import IconHomeLine from '$lib/components/icons/IconHomeLine.svelte'
  import IconLogout from '$lib/components/icons/IconLogout.svelte'
  import IconLogoutCircleRLine from '$lib/components/icons/IconLogoutCircleRLine.svelte'
  import IconOrganizationChart from '$lib/components/icons/IconOrganizationChart.svelte'
  import IconRefresh from '$lib/components/icons/IconRefresh.svelte'
  import IconUser0 from '$lib/components/icons/IconUser0.svelte'
  import IconWallet from '$lib/components/icons/IconWallet.svelte'
  import { APP_VERSION } from '$lib/constants'
  import { authStore } from '$lib/stores/auth'
  import {
    MyMessageState,
    toDisplayUserInfo,
    type DisplayUserInfo
  } from '$lib/stores/message'
  import { toastRun } from '$lib/stores/toast'
  import { Avatar, getModalStore, getToastStore } from '@skeletonlabs/skeleton'
  import { getContext, onMount } from 'svelte'
  import LeaveAccountModal from './LeaveAccountModal.svelte'

  interface Props {
    target: string
  }

  let { target }: Props = $props()

  type DisplayUserInfoEx = DisplayUserInfo & { isNameAccount?: boolean }

  const modalStore = getModalStore()
  const toastStore = getToastStore()
  let myAccounts: DisplayUserInfoEx[] = $state([])
  let myID = $state('')
  let hasNameAccounts = $state(false)

  function onWalletHandler(): void {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: WalletDetailModal,
        props: {}
      }
    })
  }

  function onLeaveAccountHandler(username: string): void {
    modalStore.trigger({
      type: 'component',
      component: {
        ref: LeaveAccountModal,
        props: {
          username,
          onConfirm: async () => {
            return toastRun(async function () {
              await authStore.nameIdentityAPI.leave_delegation(username)
              await loadMyAccounts()
            }, toastStore).finally()
          }
        }
      }
    })
  }

  function onLogoutHandler(): void {
    authStore.logout('/')
  }

  let switching = $state('')
  const globalLoading: { value: boolean } = getContext('globalLoading')
  function onSwitchHandler(info: DisplayUserInfoEx) {
    switching = info.username

    globalLoading.value = true
    toastRun(async function () {
      try {
        await authStore.switch(info.isNameAccount ? info.username : '')
      } catch (err) {
        globalLoading.value = false
        throw err
      }
    }, toastStore)
  }

  async function loadMyAccounts() {
    const myState = await MyMessageState.load()
    myID = myState.id
    const accounts = await authStore.nameIdentityAPI.get_my_accounts()
    hasNameAccounts = !!accounts.length
    myAccounts = []
    if (!hasNameAccounts) {
      return
    }
    const srcId = authStore.srcIdentity?.getPrincipal()!

    const users = await myState.batchLoadUsersInfo([
      srcId,
      ...accounts.map((account) => account.account)
    ])
    const userInfos = users.map(toDisplayUserInfo)
    for (const ac of accounts) {
      const _id = ac.account.toText()
      const info = userInfos.find(
        (info) => info._id === _id
      ) as DisplayUserInfoEx
      if (info) {
        if (!info.username) {
          info.username = ac.name
        }
        info.isNameAccount = true
        myAccounts.push(info)
      } else {
        myAccounts.push({
          _id,
          username: ac.name,
          name: ac.name,
          image: '',
          isNameAccount: true
        })
      }
    }
    myAccounts.push(userInfos[0] as DisplayUserInfoEx)
  }

  onMount(() => {
    const { abort } = toastRun(loadMyAccounts, toastStore)
    return abort
  })
</script>

<div
  class="card z-20 {hasNameAccounts
    ? 'w-72'
    : 'w-52'} bg-white px-0 py-2 shadow-lg"
  data-popup={target}
>
  <div
    class="flex flex-col items-start text-sm *:bg-surface-hover-token *:flex *:w-full *:flex-row *:gap-2 *:px-3 *:py-2"
  >
    <button type="button" onclick={onWalletHandler}>
      <span class="*:size-5"><IconWallet /></span>
      <span>Wallet</span>
    </button>
    <a type="button" href="/">
      <span class="*:size-5"><IconHomeLine /></span>
      <span>Home Page</span>
    </a>
    <a
      type="button"
      target="_blank"
      href="https://app.icpswap.com/swap?input=ryjl3-tyaaa-aaaaa-aaaba-cai&output=druyg-tyaaa-aaaaq-aactq-cai"
    >
      <span class="*:size-5"><IconExchange /></span>
      <span>Exchange PANDA</span>
    </a>
    <a type="button" target="_blank" href="https://github.com/ldclabs/ic-panda">
      <span class="*:size-5"><IconGithub /></span>
      <span>Source Code</span>
    </a>
    <button
      type="button"
      class="border-b border-surface-500/20"
      onclick={() => window.location.reload()}
    >
      <span class="*:size-5"><IconRefresh /></span>
      <span>Reload App</span>
      <span class="text-surface-500">(v{APP_VERSION})</span>
    </button>
    {#each myAccounts as info (info._id)}
      <button
        type="button"
        class="relative items-center !py-1 disabled:bg-surface-50-900-token"
        disabled={myID === info._id || switching !== ''}
        onclick={() => onSwitchHandler(info)}
      >
        {#if info.isNameAccount}
          <span class="*:size-5"><IconOrganizationChart /></span>
        {:else}
          <span class="*:size-5"><IconUser0 /></span>
        {/if}
        <Avatar
          initials={info.name}
          src={info.image}
          class="!overflow-visible"
          fill="fill-white"
          width="w-8"
        />
        <span class="truncate">
          {info.name + (info.username ? ' @' + info.username : '')}
        </span>
        {#if info.isNameAccount && myID !== info._id}
          <a
            type="button"
            href="/"
            rel="noopener noreferrer"
            class="btn absolute right-0 px-2 text-surface-500 hover:text-warning-500"
            onclick={(ev) => {
              ev.preventDefault()
              ev.stopPropagation()
              onLeaveAccountHandler(info.username)
            }}
          >
            <span class="*:size-5"><IconLogoutCircleRLine /></span>
          </a>
        {/if}
      </button>
    {/each}
    <button type="button" onclick={onLogoutHandler}>
      <span class="*:size-5"><IconLogout /></span>
      <span>Sign Out</span>
    </button>
  </div>
</div>
