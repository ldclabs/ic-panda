/** What both pages share: the build's environment, the bridge over IndexedDB, text and small DOM helpers. */
import { IC_ROOT_KEY } from '@icp-sdk/core/agent'
import { Bridge } from '../src/bridge.ts'
import { unhex } from '../src/codec.ts'
import { IdbStore } from '../src/store.ts'
import type { Environment } from '../src/protocol.ts'

declare const __DMSG_CONNECT__: {
  environment: Environment
  icHost: string
  apiOrigin: string
  handle: string
  commerce: string
  rootKey: string | null
}

export const config = __DMSG_CONNECT__

export async function openBridge(): Promise<Bridge> {
  const bridge = new Bridge(
    {
      environment: config.environment,
      relay: config.apiOrigin,
      ic: {
        host: config.icHost,
        rootKey: unhex(config.rootKey ?? IC_ROOT_KEY),
        handle: config.handle,
        commerce: config.commerce
      }
    },
    new IdbStore(config.environment)
  )
  await bridge.load()
  return bridge
}

/** The baseline of connect-v1 §4.1: WebCrypto X25519 with a non-extractable private key. */
export async function supported(): Promise<boolean> {
  try {
    await crypto.subtle.generateKey({ name: 'X25519' }, false, ['deriveBits'])
    return typeof indexedDB !== 'undefined'
  } catch {
    return false
  }
}

const ua = navigator.userAgent

/** Safari, whose storage protection applies (§4.1). */
export const isSafari = /Safari\//.test(ua) && !/Chrome\/|Chromium\/|Edg\/|Firefox\/|FxiOS|CriOS/.test(ua)

/** `Browser · System`, the pairing's label (§5.1). */
export function browserLabel(): string {
  const browser = /Edg\//.test(ua)
    ? 'Edge'
    : /Firefox\/|FxiOS/.test(ua)
      ? 'Firefox'
      : /Chrome\/|CriOS|Chromium\//.test(ua)
        ? 'Chrome'
        : isSafari
          ? 'Safari'
          : 'Browser'
  const system = /iPhone|iPad/.test(ua)
    ? 'iOS'
    : /Android/.test(ua)
      ? 'Android'
      : /Mac OS X|Macintosh/.test(ua)
        ? 'macOS'
        : /Windows/.test(ua)
          ? 'Windows'
          : /CrOS/.test(ua)
            ? 'ChromeOS'
            : /Linux/.test(ua)
              ? 'Linux'
              : 'Unknown'
  return `${browser} · ${system}`
}

const zh = {
  title: 'dMsg 连接',
  unsupported: '此浏览器不受支持。请使用 Chrome、Edge 133、Firefox 130、Safari 17 或更新版本。',
  noOpener: '这个窗口由应用打开，用来把请求交给你的 dMsg。',
  paired: '此浏览器已连接的账户',
  none: '此浏览器还没有连接任何 dMsg 账户。',
  connected: '已连接',
  ended: '应用已断开连接，可以关闭此窗口。',
  sent: '请求已发送，请在 dMsg 中确认。',
  confirm: '在 dMsg 中确认',
  approvals: '也可以在 dMsg 的审批中心或 dmsg approvals 中查看。',
  notPaired: '此浏览器还没有连接这个账户。请在 dMsg 的设置中选择“连接浏览器”。',
  openDmsg: '打开 dMsg',
  failed: '失败',
  signed: '已签名，结果已交给应用。',
  choose: '选择要使用的账户',
  continue: '继续',
  safari: 'Safari 会清除长期未使用的网站数据。点击“继续”以保留此浏览器的连接。',
  relayed: '来源由浏览器经 connect.dmsg.net 转述',
  pairTitle: '连接此浏览器',
  pairWaiting: '请在 dMsg 中确认名为下列标签的浏览器：',
  pairDone: '此浏览器已连接账户',
  pairClose: '可以关闭此页面。',
  pairMissing: '请从 dMsg 的“连接浏览器”打开此页面。',
  otherEnvironment: '链接来自其他环境。',
  invalid: '链接无效或已过期，请在 dMsg 中重新连接。'
}
const en: typeof zh = {
  title: 'dMsg Connect',
  unsupported: 'This browser is not supported. Use Chrome or Edge 133, Firefox 130, Safari 17 or later.',
  noOpener: 'Applications open this window to hand their requests to your dMsg.',
  paired: 'Accounts connected in this browser',
  none: 'This browser is not connected to any dMsg account yet.',
  connected: 'Connected',
  ended: 'The application disconnected. You can close this window.',
  sent: 'Request sent. Confirm it in dMsg.',
  confirm: 'Confirm in dMsg',
  approvals: 'You can also find it in the dMsg approval center or with dmsg approvals.',
  notPaired: 'This browser is not connected to this account. In dMsg settings, choose “Connect a browser”.',
  openDmsg: 'Open dMsg',
  failed: 'Failed',
  signed: 'Signed; the result went to the application.',
  choose: 'Choose the account to use',
  continue: 'Continue',
  safari: 'Safari clears data of sites you have not used for a while. Click Continue to keep this browser connected.',
  relayed: 'The origin is relayed by your browser through connect.dmsg.net',
  pairTitle: 'Connect this browser',
  pairWaiting: 'In dMsg, confirm the browser with this label:',
  pairDone: 'This browser is connected to account',
  pairClose: 'You can close this page.',
  pairMissing: 'Open this page from “Connect a browser” in dMsg.',
  otherEnvironment: 'The link is from another environment.',
  invalid: 'The link is invalid or has expired. Connect again in dMsg.'
}

const table = navigator.language.toLowerCase().startsWith('zh') ? zh : en
export const t = (key: keyof typeof zh) => table[key]
document.documentElement.lang = table === zh ? 'zh-Hans' : 'en'

/** An element with text and children; never HTML from data. */
export function el(tag: string, props: Record<string, string> = {}, ...children: (Node | string)[]): HTMLElement {
  const node = document.createElement(tag)
  for (const [k, v] of Object.entries(props)) node.setAttribute(k, v)
  node.append(...children)
  return node
}
