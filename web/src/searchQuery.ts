import type { Field, Query, Term } from './api'

export const FIELDS: Record<Field, { meaning: string; example: string }> = {
  name: { meaning: 'Name, as a regular expression', example: 'strbuf_*' },
  kind: { meaning: 'function, struct, invocation…', example: 'kind:struct,typedef' },
  action: { meaning: 'added, removed, modified or signature', example: 'action:signature' },
  version: { meaning: 'Changes in a version', example: 'version:2.30' },
  after: { meaning: 'Changes after a version', example: 'after:2.30' },
  before: { meaning: 'Changes before a version', example: 'before:2.40' },
  path: { meaning: 'Path start, as a regular expression', example: 'path:builtin/' },
  commit: { meaning: 'Commit hash or its start', example: 'commit:4b2d7a1' },
}

const PLAIN = /^[^\s"():-][^\s"():]*$/
const KEYWORDS = ['AND', 'OR', 'NOT']

export const term = (field: Field, value: string): Term => ({ field, value })

export function formatTerm({ field, value }: Term): string {
  const plain = PLAIN.test(value) && !KEYWORDS.includes(value)
  const quoted = plain ? value : `"${value.replace(/["\\]/g, '\\$&')}"`
  return `${field === 'name' ? '' : `${field}:`}${quoted}`
}

export function fields(query: Query): Field[] {
  if ('term' in query) {
    return [query.term.field]
  }
  if ('not' in query) {
    return fields(query.not)
  }
  const parts = 'and' in query ? query.and : query.or
  return parts.flatMap(fields)
}
