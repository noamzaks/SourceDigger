import { Anchor, Group, Select, Stack, Text, Title } from '@mantine/core'
import { IconDatabase, IconFileCode } from '@tabler/icons-react'
import { type ReactNode, useEffect } from 'react'
import { Link } from 'react-router'

import type { Revision } from '../api'
import { sourceLink } from '../links'
import { revisionAction, type useSymbol } from '../pages/symbolHistory'
import { KindBadge } from './EventRow'

interface SymbolHeaderProps {
  symbol: ReturnType<typeof useSymbol>
  action: ReactNode
}

export function SymbolHeader({ symbol, action }: SymbolHeaderProps) {
  const { events, revisions, paths, path, tooManyPaths, setParameter } = symbol
  const kinds = [...new Set(events.data?.map((event) => event.kind))]

  useEffect(() => {
    document.title = `${symbol.symbol} · ${symbol.source} · SourceDigger`
  }, [symbol.source, symbol.symbol])

  return (
    <Stack gap="md">
      <Group justify="space-between" align="center">
        <div>
          <Anchor component={Link} to={sourceLink(symbol.source)} size="sm" c="dimmed">
            <Group gap={4} wrap="nowrap">
              <IconDatabase size={14} />
              {symbol.source}
            </Group>
          </Anchor>
          <Group gap="sm" align="center">
            <Title order={2} ff="monospace" lh={1.2}>
              {symbol.symbol}
            </Title>
            {kinds.map((kind) => (
              <KindBadge key={kind} kind={kind} />
            ))}
          </Group>
          {revisions.data && <Summary revisions={revisions.data} path={path} />}
        </div>
        {action}
      </Group>
      {paths.length > 1 && (
        <Select
          placeholder={`${paths.length.toLocaleString()} files`}
          aria-label="File"
          leftSection={<IconFileCode size={16} />}
          data={paths}
          value={path}
          onChange={(value) => setParameter('path', value)}
          clearable={!tooManyPaths}
          allowDeselect={!tooManyPaths}
          searchable
          limit={100}
          maw={600}
        />
      )}
    </Stack>
  )
}

function Summary({ revisions, path }: { revisions: Revision[]; path: string | null }) {
  if (revisions.length === 0) {
    return null
  }
  const last = revisions.at(-1)!
  const changes = revisions.filter(
    (revision, index) => revisionAction(revision, revisions[index - 1]) === 'modified',
  )
  return (
    <Text c="dimmed" size="sm">
      {path && `In ${path}: `}
      Added in {revisions[0].version}
      {changes.length > 0 &&
        ` · changed in ${changes.length} ${changes.length === 1 ? 'version' : 'versions'}`}
      {last.definitions.length === 0 ? ` · removed in ${last.version}` : ' · still present'}
    </Text>
  )
}
