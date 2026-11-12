import {
  ActionIcon,
  Alert,
  Button,
  Card,
  Group,
  Loader,
  Select,
  Stack,
  Text,
  Title,
} from '@mantine/core'
import {
  IconChevronLeft,
  IconChevronRight,
  IconCode,
  IconFileCode,
  IconHistory,
  IconTag,
} from '@tabler/icons-react'
import { Link } from 'react-router'

import type { Revision } from '../api'
import { CodeBlock } from '../components/CodeBlock'
import { KindBadge, Location } from '../components/EventRow'
import { SymbolHeader } from '../components/SymbolHeader'
import { useSymbol } from './symbolHistory'

export function SymbolCodePage() {
  const symbol = useSymbol()
  const { revisions } = symbol
  const historyParameters = new URLSearchParams(symbol.query)
  historyParameters.delete('version')
  const present = revisions.data?.filter((revision) => revision.definitions.length > 0) ?? []

  return (
    <Stack gap="lg" mt="md">
      <SymbolHeader
        symbol={symbol}
        action={
          <Button
            component={Link}
            to={`..?${historyParameters}`}
            relative="path"
            variant="light"
            leftSection={<IconHistory size={16} />}
          >
            History
          </Button>
        }
      />
      {revisions.isError && <Alert color="red">{revisions.error.message}</Alert>}
      {revisions.isPending && <Loader size="sm" />}
      {revisions.data && present.length === 0 && <Text>Not found</Text>}
      {present.length > 0 && (
        <CodeViewer
          name={symbol.symbol}
          revisions={present}
          version={symbol.version}
          onSelect={(version) => symbol.setParameter('version', version)}
        />
      )}
    </Stack>
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
    <Card>
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
