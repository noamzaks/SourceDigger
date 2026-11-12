import { expect, test } from 'vitest'

import { fields, formatTerm, term } from './searchQuery'

test('writes a term back as typed, quoting what needs it', () => {
  expect(formatTerm(term('name', 'x*'))).toBe('x*')
  expect(formatTerm(term('name', 'sys_(open|close)'))).toBe('"sys_(open|close)"')
  expect(formatTerm(term('name', 'OR'))).toBe('"OR"')
  expect(formatTerm(term('kind', 'variable'))).toBe('kind:variable')
  expect(formatTerm(term('path', 'my "dir"/'))).toBe('path:"my \\"dir\\"/"')
})

test('lists the fields of a query', () => {
  expect(fields({ and: [] })).toEqual([])
  expect(
    fields({
      or: [{ term: term('name', 'x') }, { not: { and: [{ term: term('version', '1') }] } }],
    }),
  ).toEqual(['name', 'version'])
})
