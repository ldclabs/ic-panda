import assert from 'node:assert/strict'
import { test } from 'node:test'
import { textFor } from '../src/main/text.ts'
import { localeDir, locales, matchLocale, resolveLocale } from '../src/shared/locales.ts'
import en from '../src/renderer/i18n/messages/en.ts'
import zh from '../src/renderer/i18n/messages/zh.ts'
import ru from '../src/renderer/i18n/messages/ru.ts'
import ar from '../src/renderer/i18n/messages/ar.ts'
import fr from '../src/renderer/i18n/messages/fr.ts'
import es from '../src/renderer/i18n/messages/es.ts'
import zhHant from '../src/renderer/i18n/messages/zh-Hant.ts'
import hi from '../src/renderer/i18n/messages/hi.ts'
import id from '../src/renderer/i18n/messages/id.ts'
import ptBR from '../src/renderer/i18n/messages/pt-BR.ts'
import ja from '../src/renderer/i18n/messages/ja.ts'
import de from '../src/renderer/i18n/messages/de.ts'
import ko from '../src/renderer/i18n/messages/ko.ts'
import it from '../src/renderer/i18n/messages/it.ts'
import tr from '../src/renderer/i18n/messages/tr.ts'
import vi from '../src/renderer/i18n/messages/vi.ts'
import th from '../src/renderer/i18n/messages/th.ts'
import fil from '../src/renderer/i18n/messages/fil.ts'
import ms from '../src/renderer/i18n/messages/ms.ts'
import bn from '../src/renderer/i18n/messages/bn.ts'
import ur from '../src/renderer/i18n/messages/ur.ts'
import pl from '../src/renderer/i18n/messages/pl.ts'
import nl from '../src/renderer/i18n/messages/nl.ts'
import { locales as webLocales, matchLocale as webMatchLocale } from '../../dmsg_frontend/src/lib/i18n/locales.ts'

test('the stored choice wins, then the system languages in order, then English', () => {
  assert.equal(resolveLocale('fr', ['zh-CN', 'en-US']), 'fr')
  assert.equal(resolveLocale(null, ['sv-SE', 'es-MX', 'en-US']), 'es')
  assert.equal(resolveLocale('xx', ['ar-SA']), 'ar')
  assert.equal(resolveLocale(undefined, ['sv', 'fi']), 'en')
  for (const tag of ['zh', 'zh-CN', 'zh-SG', 'zh-Hans-CN', 'zh-Hans-TW']) assert.equal(matchLocale(tag), 'zh')
  for (const tag of ['zh-Hant', 'zh-TW', 'zh_HK', 'zh-MO', 'ZH_hANT_cn']) assert.equal(matchLocale(tag), 'zh-Hant')
  assert.equal(matchLocale(' RU-ru '), 'ru')
  assert.equal(matchLocale(''), null)
  assert.equal(localeDir('ar'), 'rtl')
  assert.equal(localeDir('fr'), 'ltr')
})

test('the tray, notifications and dialogs speak every interface language', () => {
  const tables = locales.map((locale) => textFor(locale))
  assert.equal(new Set(tables).size, locales.length)
  assert.equal(textFor('fr-CA'), textFor('fr'))
  assert.equal(textFor('de-DE'), textFor('de'))
  assert.equal(textFor('sv-SE'), textFor('en'))
  assert.equal(textFor('zh-HK'), textFor('zh-Hant'))
  assert.equal(textFor('pt-PT'), textFor('pt-BR'))
  for (const text of tables) {
    for (const [key, value] of Object.entries(text)) {
      const call = value as (...args: string[]) => unknown
      const shown: unknown = typeof value === 'function' ? call(...Array<string>(value.length).fill('1')) : value
      assert.ok(typeof shown === 'string' && shown.length > 0, key)
    }
  }
  for (const text of tables.slice(1)) assert.notEqual(text.state('locked'), 'locked')
})

/** The `{name}` placeholders of a label, sorted. */
const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

test('every window catalog has the labels and placeholders of the English one', () => {
  const catalogs = { zh, ru, ar, fr, es, 'zh-Hant': zhHant, 'hi': hi, 'id': id, 'pt-BR': ptBR, 'ja': ja, 'de': de, 'ko': ko, 'it': it, 'tr': tr, vi, th, fil, ms, bn, ur, pl, nl }
  for (const [name, catalog] of Object.entries(catalogs)) {
    for (const part of ['messages', 'approvalStates', 'sessionStates'] as const) {
      const source: Record<string, string> = en[part]
      const target: Record<string, string> = catalog[part]
      assert.deepEqual(Object.keys(target).sort(), Object.keys(source).sort(), `${name}.${part}`)
      for (const key of Object.keys(source)) {
        assert.ok(target[key].trim(), `${name}.${part}.${key} is empty`)
        assert.deepEqual(placeholders(target[key]), placeholders(source[key]), `${name}.${part}.${key}`)
      }
    }
  }
})


test('desktop and website use the same canonical tags and regional matching', () => {
  assert.deepEqual(locales, webLocales)
  const cases: Record<string, string | null> = {
    'hi-IN': 'hi', 'id-ID': 'id', 'in_ID': 'id', 'pt': 'pt-BR', 'pt-PT': 'pt-BR',
    'PT_br': 'pt-BR', 'ja-JP': 'ja', 'de-AT': 'de', 'de-CH': 'de', 'ko-KR': 'ko',
    'it-CH': 'it', 'tr-TR': 'tr', 'zh-HK': 'zh-Hant', 'zh-MO': 'zh-Hant',
    'zh-Hant-CN': 'zh-Hant', 'zh-Hans-HK': 'zh', 'sv-SE': null, 'vi-VN': 'vi', 'th-TH': 'th', 'fil-PH': 'fil', 'tl_PH': 'fil',
    'ms-MY': 'ms', 'ms-BN': 'ms', 'bn-BD': 'bn', 'bn-IN': 'bn', 'ur-PK': 'ur', 'ur-IN': 'ur',
    'pl-PL': 'pl', 'nl-NL': 'nl', 'nl-BE': 'nl'
  }
  for (const [tag, expected] of Object.entries(cases)) {
    assert.equal(matchLocale(tag), expected, tag)
    assert.equal(webMatchLocale(tag), expected, tag)
  }
  for (const language of locales) {
    assert.equal(matchLocale(language), language)
    assert.equal(resolveLocale(language, ['en']), language)
    assert.equal(localeDir(language), (language === 'ar' || language === 'ur') ? 'rtl' : 'ltr')
  }
})
