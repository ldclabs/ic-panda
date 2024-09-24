import { MessageCanisterAPI, type UserInfo } from '$lib/canisters/message'
import { type ChannelBasicInfo } from '$lib/canisters/messagechannel'
import { MESSAGE_CANISTER_ID } from '$lib/constants'
import { getUser2, setUser } from '$lib/stores/kvstore'
import { MessageAgent } from '$lib/stores/message_agent'
import { loadIdentity } from '$lib/utils/auth'
import { sleep } from '$lib/utils/helper'
import { tryRun } from '$lib/utils/tryrun'
import { Principal } from '@dfinity/principal'

declare let globalThis: ServiceWorkerGlobalScope

globalThis.addEventListener('notificationclick', handleNotificationClick)
const notifies = new Map<string, NotificationOptions>()

export async function notifyd() {
  let agent: MessageAgent | null = null
  let refreshStateAt = 0

  console.log('Notification service started')
  while (true) {
    const identity =
      'Notification' in globalThis &&
      Notification.permission === 'granted' &&
      (await loadIdentity())
    if (identity) {
      const now = Date.now()
      await tryRun(async () => {
        if (
          !agent ||
          agent.principal.compareTo(identity.getPrincipal()) !== 'eq' ||
          now > refreshStateAt + 20 * 60 * 1000
        ) {
          agent = await MessageAgent.with(identity)
          refreshStateAt = now
        }
        await handle(agent)
      }).finally()
      await sleep(5 * 60 * 1000)
    } else {
      // console.log('No identity')
      await sleep(20 * 60 * 1000)
    }
  }
}

async function handle(agent: MessageAgent) {
  const perm = await agent.getLocal<string>(MessageAgent.KEY_NOTIFY_PERM)
  if (!agent.api.state || perm !== 'granted') {
    return
  }

  const now = Date.now()
  const refreshMyChannelsAt =
    (await agent.getLocal<number>(MessageAgent.KEY_REFRESH_MY_CHANNELS_AT)) || 0
  if (now < refreshMyChannelsAt + 3 * 60 * 1000) {
    // message app is in active
    // console.log('Skip refresh my channels')
    return
  }

  await agent.fetchMyChannels()

  const channels = await agent.loadMyChannels()

  let unread_total = 0
  let unread_channels_total = 0
  let latest_unread_channel: ChannelBasicInfo | null = null
  for (const info of channels) {
    if (info.my_setting.unread > 0) {
      unread_total += info.my_setting.unread
      unread_channels_total += 1
      if (
        !info.my_setting.mute &&
        info.latest_message_by.compareTo(agent.api.principal) !== 'eq' &&
        info.latest_message_by.toText() != MESSAGE_CANISTER_ID &&
        (!latest_unread_channel ||
          latest_unread_channel.latest_message_at < info.latest_message_at)
      ) {
        latest_unread_channel = info
      }
    }
  }

  for (const notify of await globalThis.registration.getNotifications()) {
    notifies.delete(notify.tag)
    notify.close()
  }

  if (notifies.size < 3 && latest_unread_channel) {
    const notifyId = `${latest_unread_channel.canister.toText()}/${latest_unread_channel.id}`
    const notify = notifies.get(notifyId)
    let renotify = false
    if (notify) {
      if (
        notify.data.latest_message_by.compareTo(
          latest_unread_channel.latest_message_by
        ) === 'eq'
      ) {
        // ignore message from the same sender
        return
      }
      renotify = true
    }

    const msg = unread_total > 1 ? 'messages' : 'message'
    const chs = unread_channels_total > 1 ? 'channels' : 'channel'
    const sender = await getUserinfo(
      agent.api,
      latest_unread_channel.latest_message_by
    )
    const name = sender ? sender.username[0] : 'Someone'
    const title = `A message from ${name} in ${latest_unread_channel.name}`
    const options = {
      body: `${unread_total} unread ${msg} in ${unread_channels_total} ${chs}`,
      icon: '/favicon.ico',
      lang: 'en',
      data: latest_unread_channel,
      timestamp: now,
      tag: notifyId,
      renotify,
      vibrate: [200, 100, 200]
    }
    notifies.set(notifyId, options)
    await globalThis.registration.showNotification(title, options)
  }
}

async function getUserinfo(
  api: MessageCanisterAPI,
  principal: Principal
): Promise<UserInfo | null> {
  const now = Date.now()
  let [_, info] = await getUser2(principal)
  if (!info) {
    const users = await api.batch_get_users([principal])
    info = users[0] || null
    if (info) {
      await setUser(now, info)
    }
  }

  return info
}

async function handleNotificationClick(event: NotificationEvent) {
  const channel = event.notification.data as ChannelBasicInfo
  event.notification.close()
  if (channel) {
    const url = `/_/messages/${event.notification.tag}`
    notifies.delete(event.notification.tag)
    event.waitUntil(
      globalThis.clients
        .matchAll({
          type: 'window'
        })
        .then((clientList) => {
          for (const client of clientList) {
            // If the site is open, focus it
            return client.focus()
          }
          return globalThis.clients.openWindow(url)
        })
    )
  }
}
