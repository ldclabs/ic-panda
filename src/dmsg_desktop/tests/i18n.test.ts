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

test('the stored choice wins, then the system languages in order, then English', () => {
  assert.equal(resolveLocale('fr', ['zh-CN', 'en-US']), 'fr')
  assert.equal(resolveLocale(null, ['de-DE', 'es-MX', 'en-US']), 'es')
  assert.equal(resolveLocale('xx', ['ar-SA']), 'ar')
  assert.equal(resolveLocale(undefined, ['de', 'ja']), 'en')
  // Every Chinese variant is Simplified Chinese.
  for (const tag of ['zh', 'zh-CN', 'zh-Hans-CN', 'zh-TW', 'zh_HK']) assert.equal(matchLocale(tag), 'zh')
  assert.equal(matchLocale(' RU-ru '), 'ru')
  assert.equal(matchLocale(''), null)
  assert.equal(localeDir('ar'), 'rtl')
  assert.equal(localeDir('fr'), 'ltr')
})

test('the tray, notifications and dialogs speak every interface language', () => {
  const tables = locales.map((locale) => textFor(locale))
  assert.equal(new Set(tables).size, locales.length)
  assert.equal(textFor('fr-CA'), textFor('fr'))
  assert.equal(textFor('de-DE'), textFor('en'))
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
  const catalogs = { zh, ru, ar, fr, es }
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
