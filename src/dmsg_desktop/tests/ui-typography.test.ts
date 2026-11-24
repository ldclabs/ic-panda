import assert from 'node:assert/strict'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { test } from 'node:test'
import { typeScale } from '../src/renderer/components/ui.ts'

const renderer = new URL('../src/renderer/', import.meta.url).pathname

function sources(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) return name === 'i18n' ? [] : sources(path)
    return /\.(svelte|ts)$/.test(name) ? [path] : []
  })
}

// A size outside the scale: an arbitrary length or one of Tailwind's own names, which app.css removes.
const offScale = /\btext-(?:\[(?:length:)?[\d.]+(?:px|r?em)\]|(?:xs|sm|base|lg|[2-9]?xl)(?![\w-]))/g

test('views size text only with the type scale', () => {
  const found = sources(renderer).flatMap((file) =>
    readFileSync(file, 'utf8')
      .split('\n')
      .flatMap((line, i) => [...line.matchAll(offScale)].map((m) => `${relative(renderer, file)}:${i + 1} ${m[0]}`))
  )
  assert.deepEqual(found, [], 'Use text-title-1/2/3, text-body, text-callout, text-caption or text-code (DESIGN.md, Typography).')
})

test('the scale in ui.ts is the one app.css defines', () => {
  const css = readFileSync(join(renderer, 'app.css'), 'utf8')
  const defined = [...css.matchAll(/^\s*--text-([a-z]+(?:-\d)?):\s*[\d.]+px;/gm)].map((m) => m[1])
  assert.deepEqual(defined, [...typeScale])
})
