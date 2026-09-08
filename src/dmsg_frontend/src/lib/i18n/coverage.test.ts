import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { parse } from 'svelte/compiler'
import { describe, expect, it } from 'vitest'
import en from './messages/en'

// Product names, public handles, protocol examples and decorative text are not copy.
const invariantText = new Set([
  'dMsg',
  'ICPanda',
  'ICPanda DAO',
  'ICPanda DAO —',
  'PANDA',
  '@PANDA',
  'Internet Identity',
  'Internet Identity v2',
  'Internet Computer',
  'identity.ic0.app (legacy)',
  'Anda',
  'Anda.AI',
  'dMsg.net',
  'TokenList',
  'TokenList.ing',
  'alink',
  'al.ink/ICPanda',
  'GitHub',
  'X',
  'ICPSwap',
  'CoinGecko',
  'BNB Chain:',
  '/ dMsg',
  'AL',
  'Alex Lin',
  'vetKeys ↗',
  'release-brief.txt',
  'release-brief.txt · v1',
  '_____-_____-_____-_____-cai',
  'x'
])
const labelAttributes = new Set([
  'title',
  'aria-label',
  'alt',
  'placeholder',
  'label',
  'textLable',
  'textName'
])
function files(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? files(resolve(directory, entry.name))
      : [resolve(directory, entry.name)]
  )
}
function walk(node: any, visitor: (node: any) => boolean | void): void {
  if (!node || typeof node !== 'object' || visitor(node) === false) return
  for (const value of Object.values(node)) {
    if (Array.isArray(value)) value.forEach((child) => walk(child, visitor))
    else if (value && typeof value === 'object') walk(value, visitor)
  }
}

describe('i18n source coverage', () => {
  it('registers literal translation keys and keeps copied identifiers untranslated', () => {
    const problems: string[] = []
    for (const path of files(resolve('src')).filter((path) =>
      path.endsWith('.svelte')
    )) {
      const source = readFileSync(path, 'utf8')
      const ast = parse(source)
      walk(ast, (node) => {
        if (node.type === 'CallExpression' && node.callee?.name === '$t') {
          const key = node.arguments[0]
          if (
            key?.type === 'Literal' &&
            typeof key.value === 'string' &&
            !Object.hasOwn(en, key.value)
          ) {
            problems.push(`${path}: missing message ${key.value}`)
          }
        }
        if (node.type === 'Attribute' && node.name === 'textValue') {
          walk(node.value, (child) => {
            if (
              child.type === 'CallExpression' &&
              child.callee?.name === '$t'
            ) {
              problems.push(`${path}: clipboard payload must not be translated`)
            }
          })
        }
      })
    }
    expect(problems).toEqual([])
  })

  it('has no unregistered visible English copy or literal accessibility labels', () => {
    const problems: string[] = []
    const check = (value: string, path: string) => {
      const text = value.replace(/\s+/g, ' ').trim()
      if (/[A-Za-z]/.test(text) && !invariantText.has(text))
        problems.push(`${path}: ${text}`)
    }
    for (const path of files(resolve('src')).filter(
      (path) => path.endsWith('.svelte') && !path.includes('/icons/')
    )) {
      const ast = parse(readFileSync(path, 'utf8'))
      walk(ast['html'], (node) => {
        if (node.type === 'Attribute') {
          if (labelAttributes.has(node.name))
            for (const part of node.value || []) {
              if (part.type === 'Text') check(part.data, path)
            }
          return false
        }
        if (
          ['StyleDirective', 'Style', 'Script', 'Comment'].includes(node.type)
        )
          return false
        if (node.type === 'Text') check(node.data, path)
        return true
      })
    }
    expect(problems).toEqual([])
  })
})
