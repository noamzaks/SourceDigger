import {
  ActionIcon,
  Alert,
  Badge,
  Card,
  Group,
  Loader,
  Select,
  Stack,
  Text,
  ThemeIcon,
  Timeline,
  Title,
  UnstyledButton,
} from '@mantine/core'
import {
  IconChevronLeft,
  IconChevronRight,
  IconCode,
  IconFileCode,
  IconHistory,
  type Icon,
  IconMinus,
  IconPencil,
  IconPlus,
  IconTag,
} from '@tabler/icons-react'
import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useParams, useSearchParams } from 'react-router'

import { type Definition, type Revision, type SymbolEvent, api } from '../api'
import { BackLink } from '../components/BackLink'
import { CodeBlock } from '../components/CodeBlock'
import { CodeDiff } from '../components/CodeDiff'
import { Change, KindBadge, Location } from '../components/EventRow'

const MAXIMUM_FILES_TOGETHER = 25

type Step = 'added' | 'changed' | 'removed'

const STEPS: Record<Step, { title: string; color: string; icon: Icon }> = {
  added: { title: 'Added in', color: 'green', icon: IconPlus },
  changed: { title: 'Changed in', color: 'yellow', icon: IconPencil },
  removed: { title: 'Removed in', color: 'red', icon: IconMinus },
}

function step(revision: Revision, previous: Revision | undefined): Step {
  if (revision.definitions.length === 0) {
    return 'removed'
  }
  if (previous === undefined || previous.definitions.length === 0) {
    return 'added'
  }
  return 'changed'
}

function joined(definitions: Definition[]): string {
  return definitions.map((definition) => `// ${definition.path}\n${definition.code}\n`).join('\n')
}

export function SymbolPage() {
  const source = useParams().source!
  const symbol = useParams().symbol!
  const events = useQuery({
    queryKey: ['symbol', source, symbol],
    queryFn: () => api.symbol(source, symbol),
  })
  const paths = [
    ...new Set(
      events.data?.flatMap((event) => [
        event.definition.path,
        ...(event.previous ? [event.previous.path] : []),
      ]),
    ),
  ].sort()
  const tooManyPaths = paths.length > MAXIMUM_FILES_TOGETHER
  const [parameters, setParameters] = useSearchParams()
  const chosenPath = parameters.get('path')
  const path = chosenPath ?? (tooManyPaths ? paths[0] : null)
  const choosePath = (value: string | null) =>
    setParameters(value === null ? {} : { path: value }, { replace: true })
  const revisions = useQuery({
    queryKey: ['revisions', source, symbol, path],
    queryFn: () => api.revisions(source, symbol, path),
    enabled: events.data !== undefined,
  })

  useEffect(() => {
    document.title = `${symbol} · ${source} · SourceDigger`
  }, [source, symbol])

  const [selected, setSelected] = useState<string | null>(null)
  const kinds = [...new Set(events.data?.map((event) => event.kind))]
  const latest = useQuery({
    queryKey: ['code', source, symbol, path],
    queryFn: () => api.code(source, symbol, path),
    enabled: events.data !== undefined,
  })
  const present =
    revisions.data?.filter((revision) => revision.definitions.length > 0) ??
    (latest.data && latest.data.definitions.length > 0 ? [latest.data] : [])

  return (
    <Stack gap="lg" mt="md">
      <div>
        <BackLink to={`/sources/${encodeURIComponent(source)}`} label={source} />
        <Group gap="sm" mt={4}>
          <Title order={2} ff="monospace">
            {symbol}
          </Title>
          {kinds.map((kind) => (
            <KindBadge key={kind} kind={kind} />
          ))}
        </Group>
        {revisions.data && <Summary revisions={revisions.data} path={path} />}
      </div>
      {events.isError && <Alert color="red">{events.error.message}</Alert>}
      {paths.length > 1 && (
        <Select
          label={
            tooManyPaths
              ? `${symbol} is defined in ${paths.length.toLocaleString()} files; showing one`
              : `${symbol} is defined in ${paths.length} files`
          }
          placeholder="All files"
          leftSection={<IconFileCode size={16} />}
          data={paths}
          value={path}
          onChange={choosePath}
          clearable={!tooManyPaths}
          allowDeselect={!tooManyPaths}
          searchable
          limit={100}
          maw={600}
        />
      )}
      {revisions.isError && <Alert color="red">{revisions.error.message}</Alert>}
      {revisions.data?.length === 0 && <Text>No symbol named {symbol} was ever found.</Text>}
      {present.length > 0 && (
        <CodeViewer name={symbol} revisions={present} version={selected} onSelect={setSelected} />
      )}
      {revisions.isPending && (
        <Group gap="xs">
          <Loader size="sm" />
          <Text c="dimmed" size="sm">
            Reading the code of every version…
          </Text>
        </Group>
      )}
      {revisions.data && revisions.data.length > 0 && (
        <History
          name={symbol}
          revisions={revisions.data}
          onSelect={(version) => {
            setSelected(version)
            window.scrollTo({ top: 0, behavior: 'smooth' })
          }}
          events={(events.data ?? []).filter(
            (event) =>
              path === null || event.definition.path === path || event.previous?.path === path,
          )}
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
    (revision, index) => step(revision, revisions[index - 1]) === 'changed',
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

interface CodeViewerProps {
  name: string
  revisions: Revision[]
  version: string | null
  onSelect: (version: string) => void
}

function CodeViewer({ name, revisions, version, onSelect }: CodeViewerProps) {
  const revision = revisions.find((revision) => revision.version === version) ?? revisions.at(-1)!
  return (
    <Card withBorder p={{ base: 'md', sm: 'lg' }}>
      <Group justify="space-between" mb="md">
        <Group gap="xs">
          <IconCode size={20} />
          <Title order={4}>Code</Title>
        </Group>
        {revisions.length > 1 && (
          <VersionStepper
            versions={revisions.map((revision) => revision.version)}
            value={revision.version}
            onChange={onSelect}
          />
        )}
      </Group>
      <Stack gap="md">
        {revision.definitions.map((definition) => (
          <Stack key={`${definition.path}:${definition.line}`} gap={6}>
            <Group gap="xs">
              <IconFileCode size={16} color="var(--mantine-color-dimmed)" />
              <Location place={{ ...definition, signature: '' }} />
              <KindBadge kind={definition.kind} />
            </Group>
            <CodeBlock code={definition.code} name={name} firstLine={definition.line} />
          </Stack>
        ))}
      </Stack>
    </Card>
  )
}

interface HistoryProps {
  name: string
  revisions: Revision[]
  events: SymbolEvent[]
  onSelect: (version: string) => void
}

function History({ name, revisions, events, onSelect }: HistoryProps) {
  const newestFirst = revisions
    .map((revision, index) => ({ revision, previous: revisions[index - 1] }))
    .reverse()
  return (
    <Card withBorder p={{ base: 'md', sm: 'lg' }}>
      <Group gap="xs" mb="lg">
        <IconHistory size={20} />
        <Title order={4}>History</Title>
      </Group>
      <Timeline bulletSize={28} lineWidth={2}>
        {newestFirst.map(({ revision, previous }) => {
          const kind = step(revision, previous)
          const { title, color, icon: StepIcon } = STEPS[kind]
          const versionEvents = events.filter((event) => event.version === revision.version)
          return (
            <Timeline.Item
              key={revision.version}
              bullet={
                <ThemeIcon color={color} variant="light" radius="xl" size={28}>
                  <StepIcon size={14} />
                </ThemeIcon>
              }
              title={
                <UnstyledButton
                  onClick={() => onSelect(revision.version)}
                  disabled={revision.definitions.length === 0}
                >
                  <Text fw={600}>
                    {title} {revision.version}
                  </Text>
                </UnstyledButton>
              }
            >
              <Stack gap="xs" mt={6}>
                {versionEvents.map((event) =>
                  event.action === 'modified' ? <Change key={event.id} event={event} /> : null,
                )}
                {kind === 'added' && (
                  <Badge variant="light" color="gray" w="fit-content">
                    {revision.definitions.length}{' '}
                    {revision.definitions.length === 1 ? 'definition' : 'definitions'}
                  </Badge>
                )}
                {kind !== 'added' && previous && (
                  <CodeDiff
                    before={joined(previous.definitions)}
                    after={joined(revision.definitions)}
                    name={name}
                  />
                )}
              </Stack>
            </Timeline.Item>
          )
        })}
      </Timeline>
    </Card>
  )
}

interface VersionStepperProps {
  versions: string[]
  value: string
  onChange: (version: string) => void
}

function VersionStepper({ versions, value, onChange }: VersionStepperProps) {
  const index = versions.indexOf(value)
  return (
    <Group gap={4} wrap="nowrap">
      <ActionIcon
        variant="default"
        disabled={index <= 0}
        onClick={() => onChange(versions[index - 1])}
        aria-label="Previous change"
      >
        <IconChevronLeft size={16} />
      </ActionIcon>
      <Select
        size="xs"
        w={140}
        data={versions}
        value={value}
        onChange={(version) => version && onChange(version)}
        allowDeselect={false}
        searchable
        leftSection={<IconTag size={14} />}
        aria-label="Version"
      />
      <ActionIcon
        variant="default"
        disabled={index >= versions.length - 1}
        onClick={() => onChange(versions[index + 1])}
        aria-label="Next change"
      >
        <IconChevronRight size={16} />
      </ActionIcon>
    </Group>
  )
}
