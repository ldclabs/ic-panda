import { readdirSync, readFileSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse } from 'svelte/compiler'
import ts from 'typescript'

const renderer = resolve(
  dirname(fileURLToPath(import.meta.url)),
  '../src/renderer'
)
const textTypes = new Set([
  'text',
  'search',
  'number',
  'email',
  'url',
  'tel',
  'hidden'
])
const elements = new Set([
  'input',
  'textarea',
  'select',
  'datalist',
  'progress',
  'meter'
])
const dialogs = new Set(['alert', 'confirm', 'prompt'])

export interface Control {
  file: string
  line: number
  kind: string
  review: boolean
}

/** The shared TextField bounds its dynamic type to theme-styled text variants. */
export function sharedImplementation(control: Control) {
  return (
    control.file === 'components/TextField.svelte' &&
    control.kind === 'input:dynamic'
  )
}

function walk(node: any, visit: (node: any) => void) {
  if (!node || typeof node !== 'object') return
  visit(node)
  for (const [key, value] of Object.entries(node)) {
    if (key === 'loc') continue
    if (Array.isArray(value)) value.forEach((child) => walk(child, visit))
    else if (value && typeof value === 'object') walk(value, visit)
  }
}

/** Source occurrences, not rendered instances; comments and example strings do not count. */
export function inspectSvelte(source: string, file: string): Control[] {
  const controls: Control[] = []
  const add = (node: { start: number }, kind: string, review: boolean) =>
    controls.push({
      file,
      line: source.slice(0, node.start).split('\n').length,
      kind,
      review
    })
  walk(parse(source, { modern: true }), (node) => {
    if (node.type === 'RegularElement' && elements.has(node.name)) {
      let kind = node.name
      if (kind === 'input') {
        const attr = node.attributes.find(
          (a: any) => a.type === 'Attribute' && a.name === 'type'
        )
        const type = !attr
          ? 'text'
          : Array.isArray(attr.value) &&
              attr.value.every((v: any) => v.type === 'Text')
            ? attr.value
                .map((v: any) => v.data)
                .join('')
                .toLowerCase()
            : 'dynamic'
        kind = `input:${type}`
        add(node, kind, !textTypes.has(type))
      } else add(node, kind, kind !== 'textarea')
    }
    if (node.type !== 'CallExpression') return
    const callee = node.callee
    const name =
      callee.type === 'Identifier'
        ? callee.name
        : callee.type === 'MemberExpression' &&
            callee.object.type === 'Identifier' &&
            ['window', 'globalThis'].includes(callee.object.name)
          ? callee.computed
            ? callee.property.value
            : callee.property.name
          : null
    if (dialogs.has(name)) add(node, `dialog:${name}`, true)
  })
  return controls
}

function inspectTypeScript(source: string, file: string): Control[] {
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true)
  const controls: Control[] = []
  function visit(node: ts.Node) {
    if (ts.isCallExpression(node)) {
      const callee = node.expression
      let name: string | undefined
      if (ts.isIdentifier(callee)) name = callee.text
      else if (
        (ts.isPropertyAccessExpression(callee) ||
          ts.isElementAccessExpression(callee)) &&
        ts.isIdentifier(callee.expression) &&
        ['window', 'globalThis'].includes(callee.expression.text)
      ) {
        name = ts.isPropertyAccessExpression(callee)
          ? callee.name.text
          : ts.isStringLiteral(callee.argumentExpression)
            ? callee.argumentExpression.text
            : undefined
      }
      if (name && dialogs.has(name))
        controls.push({
          file,
          line: ast.getLineAndCharacterOfPosition(node.getStart(ast)).line + 1,
          kind: `dialog:${name}`,
          review: true
        })
    }
    ts.forEachChild(node, visit)
  }
  visit(ast)
  return controls
}

export function auditNativeControls() {
  const files = readdirSync(renderer, { recursive: true, encoding: 'utf8' })
    .filter((file) => /\.(svelte|ts)$/.test(file))
    .sort()
  const controls = files.flatMap((file) => {
    const source = readFileSync(join(renderer, file), 'utf8')
    return file.endsWith('.svelte')
      ? inspectSvelte(source, file)
      : inspectTypeScript(source, file)
  })
  return {
    files: files.length,
    svelteFiles: files.filter((f) => f.endsWith('.svelte')).length,
    controls
  }
}

export function reviewCounts(controls: Control[]) {
  const counts: Record<string, number> = {}
  for (const control of controls.filter((c) => c.review)) {
    const key = `${control.file} | ${control.kind}`
    counts[key] = (counts[key] ?? 0) + 1
  }
  return Object.fromEntries(
    Object.entries(counts).sort(([a], [b]) => a.localeCompare(b))
  )
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const result = auditNativeControls()
  if (process.argv.includes('--json'))
    console.log(JSON.stringify(result, null, 2))
  else {
    console.log(
      `Scanned ${result.svelteFiles} Svelte files and ${result.files - result.svelteFiles} TypeScript files.`
    )
    for (const c of result.controls.filter(
      (c) => c.review && !sharedImplementation(c)
    )) {
      console.log(
        `${relative(process.cwd(), join(renderer, c.file))}:${c.line} ${c.kind}`
      )
    }
    console.log(
      `${result.controls.filter((c) => c.review && !sharedImplementation(c)).length} unapproved source occurrences; ${result.controls.filter(sharedImplementation).length} shared text implementation. See docs/native-controls-audit.md.`
    )
    console.log(
      'This is a source inventory, not a visual or accessibility pass. Use --json for all controls.'
    )
  }
}
