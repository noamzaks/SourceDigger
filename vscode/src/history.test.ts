import { expect, test } from 'vitest'

import { diffHtml, historyHtml } from './history'

test('diffs keep a few lines around changes', () => {
  const before = [
    'struct a {',
    ...Array.from({ length: 10 }, (_, line) => `\tint f${line};`),
    '}',
  ].join('\n')
  const after = before.replace('int f0;', 'long f0;')
  const html = diffHtml(before, after)
  expect(html).toContain(
    '<div class="removed">-\tint f0;</div><div class="added">+\tlong f0;</div>',
  )
  expect(html).toContain('⋯ 4 unchanged lines')
  expect(diffHtml('', 'int <x>;')).toBe('<pre><div class="added">+int &#60;x&#62;;</div></pre>')
})

test('history shows each file under its path', () => {
  const html = historyHtml(
    'tls',
    [
      {
        version: '1.1',
        previous_version: '1.0',
        action: 'modified',
        files: [{ path: 'b/x.c', moved_from: 'a/x.c', before: 'int tls;', after: 'long tls;' }],
      },
    ],
    [],
    (change, file) => `command:diff?${change.previous_version},${file.moved_from}`,
  )
  expect(html).toContain('<h2>Changed in 1.1</h2>')
  expect(html).toContain('<code>a/x.c → b/x.c</code> <a href="command:diff?1.0,a/x.c">Diff</a>')
})
