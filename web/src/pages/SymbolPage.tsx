import {
  Alert,
  Button,
  Card,
  Group,
  Loader,
  SegmentedControl,
  Stack,
  Text,
  ThemeIcon,
  Timeline,
  Title,
  UnstyledButton,
} from '@mantine/core'
import { IconCode, IconFile, IconHistory } from '@tabler/icons-react'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate } from 'react-router'

import { type Action, type Change, type SymbolEvent, api } from '../api'
import { CodeDiff } from '../components/CodeDiff'
import { CommitLine } from '../components/CommitLine'
import { Change as SignatureChange, Location } from '../components/EventRow'
import { SymbolHeader } from '../components/SymbolHeader'
import { ACTION_COLORS, ACTION_ICONS, ACTION_TITLES } from '../components/symbolStyle'
import { useSymbol } from './symbolHistory'

export function SymbolPage() {
  const symbol = useSymbol()
  const navigate = useNavigate()
  const { events, path } = symbol
  const changes = useQuery({
    queryKey: ['changes', symbol.source, symbol.symbol, path],
    queryFn: () => api.changes(symbol.source, symbol.symbol, path),
    enabled: events.data !== undefined,
  })
  const codeLink = (version: string | null) => {
    const parameters = new URLSearchParams(symbol.query)
    if (version !== null) {
      parameters.set('version', version)
    }
    return `code?${parameters}`
  }

  return (
    <Stack gap="lg" mt="md">
      <SymbolHeader
        symbol={symbol}
        action={
          <Button
            component={Link}
            to={codeLink(null)}
            relative="path"
            variant="light"
            leftSection={<IconCode size={16} />}
          >
            Code
          </Button>
        }
      />
      {events.isError && <Alert color="red">{events.error.message}</Alert>}
      {changes.isError && <Alert color="red">{changes.error.message}</Alert>}
      {events.data?.length === 0 && <Text>Not found</Text>}
      {events.data && events.data.length > 0 && (
        <History
          name={symbol.symbol}
          changes={changes.data}
          onSelect={(version) => navigate(codeLink(version), { relative: 'path' })}
          events={events.data.filter(
            (event) =>
              path === null || event.definition.path === path || event.previous?.path === path,
          )}
        />
      )}
    </Stack>
  )
}

interface HistoryProps {
  name: string
  changes: Change[] | undefined
  events: SymbolEvent[]
  onSelect: (version: string) => void
}

function History({ name, changes, events, onSelect }: HistoryProps) {
  const [view, setView] = useState('Code')
  return (
    <Card>
      <Group justify="space-between" mb="lg">
        <Group gap="xs">
          <IconHistory size={20} />
          <Title order={4}>History</Title>
        </Group>
        <SegmentedControl size="xs" data={['Code', 'Signature']} value={view} onChange={setView} />
      </Group>
      {view === 'Signature' && (
        <SignatureHistory events={events.filter((event) => event.action !== 'modified')} />
      )}
      {view === 'Code' && changes === undefined && <Loader size="sm" />}
      {view === 'Code' && changes !== undefined && (
        <CodeHistory name={name} changes={changes} events={events} onSelect={onSelect} />
      )}
    </Card>
  )
}

function ActionBullet({ action }: { action: Action }) {
  const Icon = ACTION_ICONS[action]
  return (
    <ThemeIcon color={ACTION_COLORS[action]} variant="light" radius="xl" size={28}>
      <Icon size={14} />
    </ThemeIcon>
  )
}

function SignatureHistory({ events }: { events: SymbolEvent[] }) {
  return (
    <Timeline bulletSize={28} lineWidth={2}>
      {events.map((event) => (
        <Timeline.Item
          key={event.id}
          bullet={<ActionBullet action={event.action} />}
          title={
            <Text fw={600}>
              {ACTION_TITLES[event.action]} {event.version}
            </Text>
          }
        >
          <Stack gap={4} mt={6}>
            <CommitLine commit={event.commit} />
            <SignatureChange event={event} />
            <Location place={event.definition} />
          </Stack>
        </Timeline.Item>
      ))}
    </Timeline>
  )
}

interface CodeHistoryProps {
  name: string
  changes: Change[]
  events: SymbolEvent[]
  onSelect: (version: string) => void
}

function CodeHistory({ name, changes, events, onSelect }: CodeHistoryProps) {
  return (
    <Timeline bulletSize={28} lineWidth={2}>
      {changes.map((change) => {
        const commits = new Map(
          events
            .filter((event) => event.version === change.version)
            .map((event) => [event.commit.hash, event.commit]),
        )
        return (
          <Timeline.Item
            key={change.version}
            bullet={<ActionBullet action={change.action} />}
            title={
              <UnstyledButton
                onClick={() => onSelect(change.version)}
                disabled={change.action === 'removed'}
              >
                <Text fw={600}>
                  {ACTION_TITLES[change.action]} {change.version}
                </Text>
              </UnstyledButton>
            }
          >
            <Stack gap="xs" mt={6}>
              {[...commits.values()].map((commit) => (
                <CommitLine key={commit.hash} commit={commit} />
              ))}
              {change.files.map((file) => (
                <Stack key={file.path} gap={4}>
                  <Group gap={6} c="dimmed">
                    <IconFile size={14} />
                    <Text size="sm" ff="monospace">
                      {file.moved_from ? `${file.moved_from} → ${file.path}` : file.path}
                    </Text>
                  </Group>
                  <CodeDiff before={file.before} after={file.after} name={name} />
                </Stack>
              ))}
            </Stack>
          </Timeline.Item>
        )
      })}
    </Timeline>
  )
}
