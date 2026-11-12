import './highlight.css'

import type { ReactNode } from 'react'
import { Link } from 'react-router'

const KEYWORDS = new Set([
  'static',
  'inline',
  'extern',
  'const',
  'volatile',
  'register',
  'restrict',
  'struct',
  'union',
  'enum',
  'typedef',
  'sizeof',
  'return',
  'if',
  'else',
  'for',
  'while',
  'do',
  'switch',
  'case',
  'default',
  'break',
  'continue',
  'goto',
  '__inline',
  '__inline__',
  '__attribute__',
  '__init',
  '__exit',
  '__always_inline',
  '__cold',
  '__weak',
  '__maybe_unused',
  'asm',
  '__asm__',
])
const TYPES = new Set([
  'void',
  'char',
  'short',
  'int',
  'long',
  'float',
  'double',
  'signed',
  'unsigned',
  'bool',
  '_Bool',
  'size_t',
  'ssize_t',
  'u8',
  'u16',
  'u32',
  'u64',
  's8',
  's16',
  's32',
  's64',
  'uint8_t',
  'uint16_t',
  'uint32_t',
  'uint64_t',
  'int8_t',
  'int16_t',
  'int32_t',
  'int64_t',
])
const TOKEN =
  /(\/\*[\s\S]*?\*\/|\/\/[^\n]*)|(#\s*[a-z]+)|("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')|(\b\d[\w.]*)|([A-Za-z_]\w*)|(\n|[^\S\n]+|.)/g

type TokenStyle = 'comment' | 'keyword' | 'type' | 'literal'

export interface Token {
  text: string
  start: number
  style?: TokenStyle
}

export interface Mark {
  start: number
  end: number
  change: 'added' | 'removed'
}

export function tokenize(text: string): Token[] {
  const tokens: Token[] = []
  for (const match of text.matchAll(TOKEN)) {
    const [whole, comment, directive, literal, number, identifier] = match
    let style: TokenStyle | undefined
    if (comment) {
      style = 'comment'
    } else if (directive || (identifier && KEYWORDS.has(identifier))) {
      style = 'keyword'
    } else if (identifier && TYPES.has(identifier)) {
      style = 'type'
    } else if (literal || number) {
      style = 'literal'
    }
    let start = match.index
    for (const piece of whole.split(/(?<=\n)/)) {
      tokens.push({ text: piece, start, style })
      start += piece.length
    }
  }
  return tokens
}

export function splitLines(tokens: Token[]): Token[][] {
  const lines: Token[][] = [[]]
  for (const token of tokens) {
    if (token.text.endsWith('\n')) {
      lines.at(-1)!.push({ ...token, text: token.text.slice(0, -1) })
      lines.push([])
    } else {
      lines.at(-1)!.push(token)
    }
  }
  return lines
}

interface RenderOptions {
  name?: string
  symbolLink?: string
  marks?: Mark[]
}

export function renderTokens(tokens: Token[], { name, symbolLink, marks = [] }: RenderOptions) {
  const parts: ReactNode[] = []
  for (const token of tokens) {
    const mark = marks.find((mark) => token.start >= mark.start && token.start < mark.end)
    const classes = [
      token.text === name ? 'code-name' : token.style && `code-${token.style}`,
      mark && `code-${mark.change}`,
    ]
    const className = classes.filter(Boolean).join(' ')
    if (token.text === name && symbolLink) {
      parts.push(
        <Link key={token.start} className={className} to={symbolLink}>
          {token.text}
        </Link>,
      )
    } else if (className) {
      parts.push(
        <span key={token.start} className={className}>
          {token.text}
        </span>,
      )
    } else {
      parts.push(token.text)
    }
  }
  return parts
}
