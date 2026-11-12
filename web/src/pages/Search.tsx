import {
  Alert,
  Anchor,
  Autocomplete,
  Badge,
  Box,
  Button,
  Chip,
  Group,
  Kbd,
  Loader,
  Select,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useDebouncedValue, useElementSize, useHotkeys } from '@mantine/hooks'
import {
  IconArrowRight,
  IconHistory,
  IconFilter,
  IconFolder,
  IconSearch,
  IconSearchOff,
  IconSettings,
  IconTag,
} from '@tabler/icons-react'
import { keepPreviousData, useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { useWindowVirtualizer } from '@tanstack/react-virtual'
import { type FormEvent, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Link, useParams, useSearchParams } from 'react-router'

import {
  ACTIONS,
  type Action,
  type EventQuery,
  KINDS,
  type SymbolEvent,
  type Version,
  api,
} from '../api'
import { ActivityStrip } from '../components/ActivityStrip'
import { DigState } from '../components/DigState'
import { ACTION_COLORS, ACTION_ICONS, KIND_ICONS } from '../components/symbolStyle'
import { EventRow } from '../components/EventRow'
import { useSource } from '../queries'

type Row = { type: 'version'; version: string } | { type: 'event'; event: SymbolEvent }

const HEADER_HEIGHT = 56
const FILTER_BAR_PADDING = 2
const VERSION_ROW_HEIGHT = 52
const EVENT_ROW_HEIGHT = 58
const ROWS_BEFORE_END_TO_LOAD = 30
const AUTOCOMPLETE_DELAY_MILLISECONDS = 120

function parseList<T extends string>(value: string | null, all: T[]): T[] {
  if (value === null) {
    return all
  }
  return all.filter((item) => value.split(',').includes(item))
}

export function Search() {
  const source = useParams().source!
  const [parameters, setParameters] = useSearchParams()
  const query: EventQuery = {
    q: parameters.get('q') ?? '',
    kinds: parseList(parameters.get('kinds'), KINDS),
    actions: parseList(parameters.get('actions'), ACTIONS),
    since: parameters.get('since'),
    until: parameters.get('until'),
    path: parameters.get('path'),
  }

  const update = (changes: Partial<Record<keyof EventQuery, string | null>>) => {
    const next = new URLSearchParams(parameters)
    for (const [key, value] of Object.entries(changes)) {
      if (value === null || value === '') {
        next.delete(key)
      } else {
        next.set(key, value)
      }
    }
    setParameters(next)
  }
  const updateList = (key: 'kinds' | 'actions', value: string[], all: string[]) =>
    update({ [key]: value.length === all.length ? null : value.join(',') })

  const summary = useSource(source)
  const versions = useQuery({
    queryKey: ['versions', source],
    queryFn: () => api.versions(source),
  })

  const { ref: filterBarRef, height: filterBarHeight } = useElementSize()
  const [versionSlot, setVersionSlot] = useState<HTMLElement | null>(null)
  const onlyVersion = (version: string) => {
    const position = versions.data?.findIndex((candidate) => candidate.name === version) ?? 0
    update({ since: versions.data?.[position - 1]?.name ?? null, until: version })
  }

  useEffect(() => {
    document.title = query.q ? `${query.q} · ${source} · SourceDigger` : `${source} · SourceDigger`
  }, [query.q, source])

  return (
    <Stack gap="md" mt="md">
      <Group justify="space-between" align="flex-end">
        <div>
          <Title order={2}>{source}</Title>
          {summary.data && (
            <Text size="sm" c="dimmed">
              {summary.data.version_count} versions
              {summary.data.first_version &&
                ` · ${summary.data.first_version} → ${summary.data.last_version}`}
            </Text>
          )}
        </div>
        <Group gap="md">
          {summary.data && <DigState dig={summary.data.last_dig} />}
          <Button
            component={Link}
            to={`/sources/${encodeURIComponent(source)}/settings`}
            variant="default"
            size="xs"
            leftSection={<IconSettings size={14} />}
          >
            Settings
          </Button>
        </Group>
      </Group>
      {versions.data && versions.data.length > 1 && (
        <ActivityStrip
          versions={versions.data}
          since={query.since}
          until={query.until}
          onSelect={onlyVersion}
        />
      )}
      {summary.isError && <Alert color="red">{summary.error.message}</Alert>}

      <Box
        ref={filterBarRef}
        pos={{ base: 'static', sm: 'sticky' }}
        top={HEADER_HEIGHT}
        py="sm"
        bg="var(--mantine-color-body)"
        style={{ zIndex: 10, borderBottom: '1px solid var(--mantine-color-default-border)' }}
      >
        <SearchBox key={query.q} source={source} value={query.q} onSearch={(q) => update({ q })} />
        <Group gap="lg" mt="sm">
          <Chip.Group
            multiple
            value={query.kinds}
            onChange={(value) => updateList('kinds', value, KINDS)}
          >
            <Group gap={6}>
              {KINDS.map((kind) => {
                const Icon = KIND_ICONS[kind]
                return (
                  <Chip key={kind} value={kind} size="xs" variant="outline">
                    <Group gap={4} wrap="nowrap">
                      <Icon size={12} />
                      {kind}
                    </Group>
                  </Chip>
                )
              })}
            </Group>
          </Chip.Group>
          <Chip.Group
            multiple
            value={query.actions}
            onChange={(value) => updateList('actions', value, ACTIONS)}
          >
            <Group gap={6}>
              {ACTIONS.map((action: Action) => {
                const Icon = ACTION_ICONS[action]
                return (
                  <Chip key={action} value={action} size="xs" color={ACTION_COLORS[action]}>
                    <Group gap={4} wrap="nowrap">
                      <Icon size={12} />
                      {action}
                    </Group>
                  </Chip>
                )
              })}
            </Group>
          </Chip.Group>
          {versions.data && versions.data.length > 1 && (
            <VersionRange
              versions={versions.data}
              since={query.since}
              until={query.until}
              onChange={update}
            />
          )}
          <PathFilter key={query.path} value={query.path} onChange={(path) => update({ path })} />
          <Box ref={setVersionSlot} ml="auto" visibleFrom="sm" />
        </Group>
      </Box>

      <Results
        source={source}
        query={query}
        onlyVersion={onlyVersion}
        stickyTop={HEADER_HEIGHT + filterBarHeight + FILTER_BAR_PADDING}
        versionSlot={versionSlot}
      />
    </Stack>
  )
}

function PathFilter({
  value,
  onChange,
}: {
  value: string | null
  onChange: (path: string | null) => void
}) {
  const [input, setInput] = useState(value ?? '')
  const apply = () => onChange(input.trim() || null)
  return (
    <TextInput
      size="xs"
      w={220}
      placeholder="Any file or directory"
      leftSection={<IconFolder size={14} />}
      value={input}
      onChange={(event) => setInput(event.currentTarget.value)}
      onBlur={apply}
      onKeyDown={(event) => event.key === 'Enter' && apply()}
      spellCheck={false}
      aria-label="Only files under this path"
    />
  )
}

interface VersionRangeProps {
  versions: Version[]
  since: string | null
  until: string | null
  onChange: (changes: { since?: string | null; until?: string | null }) => void
}

function VersionRange({ versions, since, until, onChange }: VersionRangeProps) {
  const names = versions.map((version) => version.name).reverse()
  return (
    <Group gap={6} wrap="nowrap">
      <Select
        size="xs"
        w={150}
        placeholder="First version"
        leftSection={<IconTag size={14} />}
        data={names}
        value={since}
        onChange={(version) => onChange({ since: version })}
        searchable
        clearable
        aria-label="Changes after this version"
      />
      <IconArrowRight size={14} color="var(--mantine-color-dimmed)" />
      <Select
        size="xs"
        w={150}
        placeholder="Latest version"
        leftSection={<IconTag size={14} />}
        data={names}
        value={until}
        onChange={(version) => onChange({ until: version })}
        searchable
        clearable
        aria-label="Changes up to this version"
      />
    </Group>
  )
}

interface SearchBoxProps {
  source: string
  value: string
  onSearch: (q: string) => void
}

function SearchBox({ source, value, onSearch }: SearchBoxProps) {
  const [input, setInput] = useState(value)
  const [debounced] = useDebouncedValue(input.trim(), AUTOCOMPLETE_DELAY_MILLISECONDS)
  const inputRef = useRef<HTMLInputElement>(null)
  useHotkeys([['/', () => inputRef.current?.focus()]])

  const names = useQuery({
    queryKey: ['names', source, debounced],
    queryFn: () => api.names(source, debounced),
    enabled: debounced !== '' && debounced !== value,
    placeholderData: keepPreviousData,
  })

  const submit = (event: FormEvent) => {
    event.preventDefault()
    onSearch(input.trim())
  }

  return (
    <form onSubmit={submit}>
      <Autocomplete
        ref={inputRef}
        size="md"
        value={input}
        onChange={setInput}
        onOptionSubmit={(name) => {
          setInput(name)
          onSearch(name)
        }}
        data={debounced === '' ? [] : (names.data ?? [])}
        filter={({ options }) => options}
        limit={20}
        leftSection={<IconSearch size={18} />}
        rightSection={names.isFetching ? <Loader size="xs" /> : <Kbd size="xs">/</Kbd>}
        placeholder="Search symbols: kmalloc, vm_*, sys_(open|close)"
        autoFocus
        spellCheck={false}
        autoCapitalize="off"
        autoComplete="off"
        comboboxProps={{ transitionProps: { duration: 0 } }}
      />
    </form>
  )
}

interface ResultsProps {
  source: string
  query: EventQuery
  onlyVersion: (version: string) => void
  stickyTop: number
  versionSlot: HTMLElement | null
}

function Results({ source, query, onlyVersion, stickyTop, versionSlot }: ResultsProps) {
  const events = useInfiniteQuery({
    queryKey: ['events', source, query],
    queryFn: ({ pageParam }) => api.events(source, query, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next,
  })

  const rows = useMemo(() => {
    const rows: Row[] = []
    let version: string | null = null
    for (const page of events.data?.pages ?? []) {
      for (const event of page.events) {
        if (event.version !== version) {
          version = event.version
          rows.push({ type: 'version', version })
        }
        rows.push({ type: 'event', event })
      }
    }
    return rows
  }, [events.data])

  const listRef = useRef<HTMLDivElement>(null)
  const [scrollMargin, setScrollMargin] = useState(0)
  const hasRows = rows.length > 0
  useLayoutEffect(() => {
    const list = listRef.current
    if (!list) {
      return
    }
    const measure = () => setScrollMargin(list.getBoundingClientRect().top + window.scrollY)
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(document.body)
    return () => observer.disconnect()
  }, [hasRows])

  const virtualizer = useWindowVirtualizer({
    count: rows.length,
    estimateSize: (index) =>
      rows[index].type === 'version' ? VERSION_ROW_HEIGHT : EVENT_ROW_HEIGHT,
    overscan: 12,
    scrollMargin,
  })
  const items = virtualizer.getVirtualItems()
  const lastIndex = items.at(-1)?.index ?? 0

  useEffect(() => {
    if (
      lastIndex >= rows.length - ROWS_BEFORE_END_TO_LOAD &&
      events.hasNextPage &&
      !events.isFetchingNextPage
    ) {
      events.fetchNextPage()
    }
  }, [lastIndex, rows.length, events])

  if (events.isError) {
    return (
      <Alert color="red" title="Search failed">
        {events.error.message}
      </Alert>
    )
  }
  if (events.isPending) {
    return (
      <Group justify="center" mt="xl">
        <Loader />
      </Group>
    )
  }
  if (rows.length === 0) {
    return (
      <Stack align="center" mt="xl" gap={4}>
        <IconSearchOff size={32} color="var(--mantine-color-dimmed)" />
        <Text fw={500}>No symbols found</Text>
        <Text size="sm" c="dimmed">
          The search is a regular expression over the whole name. Try{' '}
          <Anchor component={Link} to={`?q=${encodeURIComponent(`${query.q}*`)}`}>
            {query.q}*
          </Anchor>
          .
        </Text>
      </Stack>
    )
  }

  const symbolLink = (name: string) =>
    `/sources/${encodeURIComponent(source)}/symbols/${encodeURIComponent(name)}`

  const firstVisible = items.find((item) => item.end > window.scrollY + stickyTop)
  let currentVersion: string | null = null
  if (firstVisible && rows[firstVisible.index].type === 'event') {
    for (let index = firstVisible.index; index >= 0 && currentVersion === null; index -= 1) {
      const row = rows[index]
      currentVersion = row.type === 'version' ? row.version : null
    }
  }

  const names = new Set(rows.flatMap((row) => (row.type === 'event' ? [row.event.name] : [])))
  const onlyName = names.size === 1 && !events.hasNextPage ? [...names][0] : null

  return (
    <>
      {onlyName && (
        <Group justify="flex-end" mb="sm">
          <Button
            component={Link}
            to={symbolLink(onlyName)}
            variant="light"
            leftSection={<IconHistory size={16} />}
            rightSection={<IconArrowRight size={14} />}
          >
            Code and history of {onlyName}
          </Button>
        </Group>
      )}
      {currentVersion &&
        versionSlot &&
        createPortal(
          <Badge
            size="lg"
            variant="default"
            radius="sm"
            tt="none"
            leftSection={<IconTag size={14} />}
          >
            {currentVersion}
          </Badge>,
          versionSlot,
        )}
      <div ref={listRef} style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
        {items.map((item) => {
          const row = rows[item.index]
          return (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${item.start - scrollMargin}px)`,
              }}
            >
              {row.type === 'version' ? (
                <Group justify="space-between" pt="lg" pb={6} px="xs">
                  <Group gap={6}>
                    <IconTag size={18} color="var(--mantine-color-dimmed)" />
                    <Title order={4}>{row.version}</Title>
                  </Group>
                  {query.since === null && query.until === null && (
                    <Button
                      variant="subtle"
                      size="compact-xs"
                      leftSection={<IconFilter size={12} />}
                      onClick={() => onlyVersion(row.version)}
                    >
                      Only this version
                    </Button>
                  )}
                </Group>
              ) : (
                <EventRow event={row.event} symbolLink={symbolLink(row.event.name)} />
              )}
            </div>
          )
        })}
      </div>
      {events.isFetchingNextPage && (
        <Group justify="center" py="md">
          <Loader size="sm" />
        </Group>
      )}
    </>
  )
}
