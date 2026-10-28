/**
 * Resolve text for native menus, notifications, update prompts and OS dialogs.
 * Page text belongs to renderer/i18n; both use the shared locale selection.
 */
import { defaultLocale, matchLocale, type Locale } from '../shared/locales.ts'
import en, { type Text } from './messages/en.ts'
import zh from './messages/zh.ts'
import zhHant from './messages/zh-Hant.ts'
import ru from './messages/ru.ts'
import ar from './messages/ar.ts'
import fr from './messages/fr.ts'
import es from './messages/es.ts'
import hi from './messages/hi.ts'
import id from './messages/id.ts'
import ptBR from './messages/pt-BR.ts'
import ja from './messages/ja.ts'
import de from './messages/de.ts'
import ko from './messages/ko.ts'
import it from './messages/it.ts'
import tr from './messages/tr.ts'
import vi from './messages/vi.ts'
import th from './messages/th.ts'
import fil from './messages/fil.ts'
import ms from './messages/ms.ts'
import bn from './messages/bn.ts'
import ur from './messages/ur.ts'
import pl from './messages/pl.ts'
import nl from './messages/nl.ts'

const texts: Record<Locale, Text> = {
  en,
  zh,
  'zh-Hant': zhHant,
  ru,
  ar,
  fr,
  es,
  hi,
  id,
  'pt-BR': ptBR,
  ja,
  de,
  ko,
  it,
  tr,
  vi,
  th,
  fil,
  ms,
  bn,
  ur,
  pl,
  nl
}

export type { Text }

/** The table for a locale such as `fr` or `zh-CN`; English for any other. */
export function textFor(locale: string): Text {
  return texts[matchLocale(locale) ?? defaultLocale]
}
