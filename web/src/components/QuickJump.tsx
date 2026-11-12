import { Group, Kbd, Text, UnstyledButton } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { Spotlight, spotlight } from '@mantine/spotlight'
import { IconDatabase, IconCode, IconSearch } from '@tabler/icons-react'
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router'

import { api } from '../api'
import { sourcesQuery } from '../queries'

const DELAY_MILLISECONDS = 120

export function QuickJumpButton() {
  return (
    <UnstyledButton
      onClick={spotlight.open}
      px="sm"
      h={34}
      w={260}
      visibleFrom="sm"
      style={{
        border: '1px solid var(--mantine-color-default-border)',
        borderRadius: 'var(--mantine-radius-md)',
      }}
    >
      <Group justify="space-between" wrap="nowrap">
        <Group gap={8} wrap="nowrap">
          <IconSearch size={16} color="var(--mantine-color-dimmed)" />
          <Text size="sm" c="dimmed">
            Jump to…
          </Text>
        </Group>
        <Kbd size="xs">Ctrl + K</Kbd>
      </Group>
    </UnstyledButton>
  )
}

export function QuickJump() {
  const navigate = useNavigate()
  const source = useParams().source
  const [query, setQuery] = useState('')
  const [debounced] = useDebouncedValue(query.trim(), DELAY_MILLISECONDS)
  const [opened, setOpened] = useState(false)
  const sources = useQuery({ ...sourcesQuery, enabled: opened })
  const names = useQuery({
    queryKey: ['names', source, debounced],
    queryFn: () => api.names(source!, debounced),
    enabled: source !== undefined && debounced !== '',
  })

  const matchingSources = (sources.data ?? []).filter((candidate) =>
    candidate.name.toLowerCase().includes(query.trim().toLowerCase()),
  )
  const sourcePath = (name: string) => `/sources/${encodeURIComponent(name)}`

  return (
    <Spotlight.Root
      query={query}
      onQueryChange={setQuery}
      shortcut="mod + K"
      onSpotlightOpen={() => setOpened(true)}
    >
      <Spotlight.Search
        placeholder={source ? `Symbols in ${source}, or a source…` : 'A source…'}
        leftSection={<IconSearch size={18} />}
      />
      <Spotlight.ActionsList>
        {source && debounced !== '' && (
          <Spotlight.ActionsGroup label={`Symbols in ${source}`}>
            <Spotlight.Action
              leftSection={<IconSearch size={16} />}
              label={`Search changes of ${debounced}`}
              onClick={() =>
                navigate(`${sourcePath(source)}?${new URLSearchParams({ q: debounced })}`)
              }
            />
            {names.data?.map((name) => (
              <Spotlight.Action
                key={name}
                leftSection={<IconCode size={16} />}
                label={name}
                styles={{ actionLabel: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
                onClick={() =>
                  navigate(`${sourcePath(source)}/symbols/${encodeURIComponent(name)}`)
                }
              />
            ))}
          </Spotlight.ActionsGroup>
        )}
        {matchingSources.length > 0 && (
          <Spotlight.ActionsGroup label="Sources">
            {matchingSources.map((candidate) => (
              <Spotlight.Action
                key={candidate.id}
                leftSection={<IconDatabase size={16} />}
                label={candidate.name}
                description={`${candidate.version_count} versions`}
                onClick={() => navigate(sourcePath(candidate.name))}
              />
            ))}
          </Spotlight.ActionsGroup>
        )}
        {matchingSources.length === 0 && !names.data?.length && (
          <Spotlight.Empty>Nothing found</Spotlight.Empty>
        )}
      </Spotlight.ActionsList>
    </Spotlight.Root>
  )
}
