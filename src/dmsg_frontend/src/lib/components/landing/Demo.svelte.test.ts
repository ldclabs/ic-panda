import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createRawSnippet, flushSync, mount, unmount, type Component } from 'svelte'
import { setLocale } from '$lib/i18n'
import Brand from './Brand.svelte'
import Demo from './Demo.svelte'

vi.mock('$app/navigation', () => ({ replaceState: vi.fn() }))
vi.mock('$app/state', () => ({ page: { state: {} } }))

let target: HTMLDivElement
let component: ReturnType<typeof mount> | undefined

beforeEach(async () => {
  vi.clearAllMocks()
  await setLocale('en', false)
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = undefined
  target.remove()
  vi.restoreAllMocks()
})

describe('Demo', () => {
  it('keeps static body searchable while closed and defers figure loading until opened', async () => {
    const loadFigure = vi.fn(async () => ({ default: Brand as Component }))
    const children = createRawSnippet(() => ({
      render: () => '<p class="demo-intro">Findable intro copy</p>'
    }))
    const props = $state({
      title: 'Example demo',
      open: false,
      children,
      loadFigure
    })

    component = mount(Demo, { target, props })
    flushSync()

    expect(target.querySelector('.demo-intro')).toHaveTextContent(
      'Findable intro copy'
    )
    expect(loadFigure).not.toHaveBeenCalled()
    expect(target.querySelector('.brand')).toBeNull()

    props.open = true
    flushSync()

    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('.brand')).not.toBeNull()
    })
    expect(loadFigure).toHaveBeenCalledTimes(1)
  })

  it('caches the loaded figure across locale switches and reopenings without remounting', async () => {
    const loadFigure = vi.fn(async () => ({ default: Brand as Component }))
    const props = $state({
      title: 'Example demo',
      open: true,
      loadFigure
    })

    component = mount(Demo, { target, props })
    flushSync()

    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('.brand')).not.toBeNull()
    })
    const initialNode = target.querySelector('.brand')

    await setLocale('zh', false)
    flushSync()
    expect(loadFigure).toHaveBeenCalledTimes(1)
    expect(target.querySelector('.brand')).toBe(initialNode)

    props.open = false
    flushSync()
    props.open = true
    flushSync()
    expect(loadFigure).toHaveBeenCalledTimes(1)
    expect(target.querySelector('.brand')).toBe(initialNode)
  })

  it('reports load failures in an alert and retries when closed and reopened', async () => {
    const loadFigure = vi
      .fn<() => Promise<{ default: Component }>>()
      .mockRejectedValueOnce(new TypeError('chunk missing'))
      .mockResolvedValueOnce({ default: Brand as Component })
    const props = $state({
      title: 'Example demo',
      open: true,
      loadFigure
    })

    component = mount(Demo, { target, props })
    flushSync()

    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('[role="alert"]')).toHaveTextContent(
        'Could not load this page.'
      )
    })
    expect(target.querySelector('button[type="button"]')).toHaveTextContent(
      'Try again'
    )

    props.open = false
    flushSync()
    props.open = true
    flushSync()

    await vi.waitFor(() => {
      flushSync()
      expect(target.querySelector('.brand')).not.toBeNull()
    })
    expect(loadFigure).toHaveBeenCalledTimes(2)
    expect(target.querySelector('[role="alert"]')).toBeNull()
  })
})
