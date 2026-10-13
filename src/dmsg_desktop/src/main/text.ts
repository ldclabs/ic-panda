/**
 * Text of the tray, notifications, update prompts and file dialogs in
 * Simplified Chinese or English. The main process picks the table from the
 * app's locale, as the windows pick theirs from the same system language
 * (`renderer/i18n.ts`).
 */

const zhStates: Record<string, string> = {
  connecting: '连接中',
  unknown: '未知',
  logged_out: '未登录',
  pairing_required: '等待配对',
  no_account: '尚未创建账户',
  locked: '已锁定',
  root_required: '需要内容根',
  unlocked: '已解锁'
}

const enStates: Record<string, string> = {
  connecting: 'connecting',
  unknown: 'unknown',
  logged_out: 'logged out',
  pairing_required: 'waiting to pair',
  no_account: 'no account yet',
  locked: 'locked',
  root_required: 'content root needed',
  unlocked: 'unlocked'
}

const zh = {
  state: (state: string) => zhStates[state] ?? state,
  waiting: (n: number) => `${n} 个请求待确认`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `，${n} 个待确认` : ''}`,
  open: '打开 dMsg',
  lock: '锁定',
  openAtLogin: '登录时打开',
  quit: '退出',
  checkUpdates: '检查更新',
  checking: '正在检查更新…',
  downloading: '正在下载更新…',
  restartToUpdate: (version: string) => `重启并更新到 ${version}`,
  requestWaiting: '有请求等待你确认。',
  eventsArrived: (n: number) => `Agent 收到 ${n} 条新事件。`,
  refreshDue: '有 Agent 的事件订阅需要续订。',
  updateLater: 'dMsg 会在进行中的操作完成后更新',
  updateBusy: (busy: string) => `仍在进行：${busy}。解锁 dMsg 让它完成，然后再选“重启并更新”。`,
  inFlightApprovals: (n: number) => `${n} 个已批准、结果尚未确定的请求`,
  inFlightAccount: '一次结果未知的账户更新',
  inFlightRoot: '一次换根',
  separator: '、',
  allFiles: '所有文件',
  migrationFile: 'dMsg 迁移文件',
  frozenFile: '冻结历史',
  bundleFile: '证据包',
  pickBundle: '选择要核验的证据包',
  pickOriginal: '选择证据包对应的原文件',
  pickAction: '选择应用准备的动作（CBOR）',
  pickAvatar: '选择公开头像（PNG、JPEG 或 WebP，最大 512 KiB）',
  imageFile: '图片',
  newMessages: 'dMsg 有新的频道消息。',
  newLetter: 'dMsg 有新来信。'
}

type Text = typeof zh

const en: Text = {
  state: (state: string) => enStates[state] ?? state,
  waiting: (n: number) => `${n} request(s) waiting`,
  tooltip: (version: string, state: string, n: number) => `dMsg ${version} — ${state}${n ? `, ${n} waiting` : ''}`,
  open: 'Open dMsg',
  lock: 'Lock',
  openAtLogin: 'Open at Login',
  quit: 'Quit',
  checkUpdates: 'Check for Updates',
  checking: 'Checking for Updates…',
  downloading: 'Downloading Update…',
  restartToUpdate: (version: string) => `Restart to Update to ${version}`,
  requestWaiting: 'A request is waiting for your approval.',
  eventsArrived: (n: number) => `Your agents received ${n} new event(s).`,
  refreshDue: "An agent's event subscription is due for renewal.",
  updateLater: 'dMsg will update when its work in flight settles',
  updateBusy: (busy: string) => `Still in flight: ${busy}. Unlock dMsg so it can finish, then choose Restart to Update again.`,
  inFlightApprovals: (n: number) => `${n} approved request(s) whose outcome is not settled`,
  inFlightAccount: 'an account update whose outcome is not known',
  inFlightRoot: 'a root rotation',
  separator: ', ',
  allFiles: 'All files',
  migrationFile: 'dMsg migration',
  frozenFile: 'Frozen history',
  bundleFile: 'Attestation bundle',
  pickBundle: 'Choose the bundle to verify',
  pickOriginal: 'Choose the original file of the bundle',
  pickAction: 'Choose the action the application prepared (CBOR)',
  pickAvatar: 'Choose a public avatar (PNG, JPEG or WebP, at most 512 KiB)',
  imageFile: 'Images',
  newMessages: 'New channel messages in dMsg.',
  newLetter: 'A new letter arrived in dMsg.'
}

export type { Text }

/** The table for a locale such as `zh-CN` or `en-US`. */
export function textFor(locale: string): Text {
  return locale.toLowerCase().startsWith('zh') ? zh : en
}
