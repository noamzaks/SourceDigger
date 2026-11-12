import { expect, test } from 'vitest'

import { type SymbolEvent, describe, matchingSource, repositoryKey } from './sourcedigger'

test('remotes match sources whatever their form', () => {
  const key = 'github.com/git/git'
  expect(repositoryKey('https://github.com/git/git.git')).toBe(key)
  expect(repositoryKey('git@github.com:git/git.git')).toBe(key)
  expect(repositoryKey('ssh://git@GitHub.com/git/git/')).toBe(key)
  expect(repositoryKey('https://example.com:8443/a.git')).toBe('example.com:8443/a')
  const sources = [
    { name: 'musl', url: 'https://git.musl-libc.org/git/musl' },
    { name: 'git', url: 'https://github.com/git/git.git' },
  ]
  expect(matchingSource(sources, ['', 'git@github.com:git/git.git'])?.name).toBe('git')
  expect(matchingSource(sources, ['https://github.com/torvalds/linux'])).toBeUndefined()
})

function event(
  version: string,
  action: SymbolEvent['action'],
  kind: string,
  signature: string,
  previous: string | null = null,
): SymbolEvent {
  const line = kind === 'invocation' ? 9 : 1
  const place = (text: string) => ({ path: 'kernel/rcu.c', line, signature: text })
  return {
    version,
    commit: { hash: `${version}0000000000`, summary: `Release ${version}` },
    action,
    kind,
    definition: place(signature),
    previous: previous === null ? null : place(previous),
  }
}

test('describes what a symbol is now and how it got there', () => {
  const newestFirst = [
    event(
      '2.6.18',
      'signature',
      'invocation',
      'EXPORT_SYMBOL_GPL(call_rcu)',
      'EXPORT_SYMBOL(call_rcu)',
    ),
    event('2.6.14', 'modified', 'function', 'void call_rcu(void)', 'void call_rcu(void)'),
    event('2.6.12', 'added', 'invocation', 'EXPORT_SYMBOL(call_rcu)'),
    event('2.6.12', 'added', 'function', 'void call_rcu(void)'),
  ]
  expect(describe('call_rcu', newestFirst)).toBe(
    [
      '**call_rcu** · invocation, function',
      '',
      'added in 2.6.12 · changed in 2 versions',
      '',
      '```c',
      'void call_rcu(void)',
      'EXPORT_SYMBOL_GPL(call_rcu)',
      '```',
    ].join('\n'),
  )
  const removed = [
    event('3.0', 'removed', 'function', 'void call_rcu(void)'),
    ...newestFirst.slice(1, 2),
    newestFirst[3],
  ]
  expect(describe('call_rcu', removed)).toContain('removed in 3.0')
})
