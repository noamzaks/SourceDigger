import { Box, Text } from '@mantine/core'
import { diffLines } from 'diff'
import type { ReactNode } from 'react'

import { CodeBox } from './CodeBox'
import { renderTokens, tokenize } from './highlight'

const CONTEXT_LINES = 3

type Line = { kind: 'same' | 'added' | 'removed'; text: string }

const BACKGROUNDS = {
  same: undefined,
  added: 'var(--mantine-color-green-light)',
  removed: 'var(--mantine-color-red-light)',
}
const SIGNS = { same: ' ', added: '+', removed: '−' }

function diffToLines(before: string, after: string): Line[] {
  const lines: Line[] = []
  for (const part of diffLines(before, after)) {
    const kind = part.added ? 'added' : part.removed ? 'removed' : 'same'
    for (const text of part.value.replace(/\n$/, '').split('\n')) {
      lines.push({ kind, text })
    }
  }
  return lines
}

export function CodeDiff({ before, after, name }: { before: string; after: string; name: string }) {
  const lines = diffToLines(before, after)
  const near = lines.map((_, index) =>
    lines
      .slice(Math.max(0, index - CONTEXT_LINES), index + CONTEXT_LINES + 1)
      .some((line) => line.kind !== 'same'),
  )
  const rows: ReactNode[] = []
  let hidden = 0
  lines.forEach((line, index) => {
    if (!near[index]) {
      hidden += 1
      return
    }
    if (hidden > 0) {
      rows.push(<Unchanged key={`hidden-${index}`} count={hidden} />)
      hidden = 0
    }
    rows.push(
      <Box key={index} bg={BACKGROUNDS[line.kind]} px="xs" style={{ whiteSpace: 'pre' }}>
        <Text span inherit c="dimmed" mr="xs" style={{ userSelect: 'none' }}>
          {SIGNS[line.kind]}
        </Text>
        {renderTokens(tokenize(line.text), { name })}
      </Box>,
    )
  })
  if (hidden > 0) {
    rows.push(<Unchanged key="hidden-end" count={hidden} />)
  }
  return <CodeBox>{rows}</CodeBox>
}

function Unchanged({ count }: { count: number }) {
  return (
    <Text c="dimmed" inherit px="xs" py={2}>
      ⋯ {count} unchanged {count === 1 ? 'line' : 'lines'}
    </Text>
  )
}
