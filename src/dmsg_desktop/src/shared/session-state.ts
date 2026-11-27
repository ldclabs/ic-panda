import type { Locale } from './locales'

const moved: Record<Locale, string> = {
  en: 'Copied from another PC',
  zh: '从另一台电脑复制而来',
  'zh-Hant': "從另一台電腦複製而來",
  ru: 'Скопировано с другого компьютера',
  ar: 'منسوخ من حاسوب آخر',
  fr: 'Copié depuis un autre PC',
  es: 'Copiado desde otro PC',
  hi: "दूसरे PC से कॉपी किया गया",
  id: "Disalin dari PC lain",
  'pt-BR': "Copiado de outro PC",
  ja: "別の PC からコピー",
  de: "Von einem anderen PC kopiert",
  ko: "다른 PC에서 복사됨",
  it: "Copiato da un altro PC",
  tr: "Başka bir bilgisayardan kopyalandı",
  vi: "Sao chép từ máy tính khác",
  th: "คัดลอกมาจากคอมพิวเตอร์เครื่องอื่น",
  fil: "Kinopya mula sa ibang PC",
  ms: "Disalin dari PC lain",
  bn: "অন্য PC থেকে কপি করা",
  ur: "دوسرے PC سے کاپی شدہ",
  pl: "Skopiowano z innego komputera",
  nl: "Gekopieerd van een andere pc"
}

/** Windows migration state shared by the system tray and renderer. */
export function windowsMovedState(state: string, locale: Locale, platform: string): string | undefined {
  return state === 'moved' && platform === 'win32' ? moved[locale] : undefined
}
