import {
  ActionIcon,
  Alert,
  Anchor,
  Autocomplete,
  Badge,
  Box,
  Button,
  Group,
  Kbd,
  Loader,
  Popover,
  Stack,
  Text,
  Title,
  UnstyledButton,
} from '@mantine/core'
import { useDebouncedValue, useElementSize, useHotkeys } from '@mantine/hooks'
import {
  type Icon as TablerIcon,
  IconArrowRight,
  IconFilter,
  IconHelp,
  IconHistory,
  IconFilterOff,
  IconSearch,
  IconSearchOff,
  IconSettings,
  IconTag,
} from '@tabler/icons-react'
import { keepPreviousData, useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { useWindowVirtualizer } from '@tanstack/react-virtual'
import {
  type FormEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { createPortal } from 'react-dom'
import { Link, useParams, useSearchParams } from 'react-router'

import {
  ACTIONS,
  type Commit,
  type Field,
  KINDS,
  type Query,
  type SymbolEvent,
  type Version,
  api,
} from '../api'
import { ActivityStrip } from '../components/ActivityStrip'
import { CommitLine } from '../components/CommitLine'
import { DigState } from '../components/DigState'
import { EventRow } from '../components/EventRow'
import { FIELD_ICONS } from '../components/symbolStyle'
import { useSource } from '../queries'
import { searchLink, settingsLink, symbolLink } from '../links'
import { FIELDS, fields, formatTerm, term } from '../searchQuery'

type Row =
  | { type: 'version'; version: string }
  | { type: 'commit'; commit: Commit }
  | { type: 'event'; event: SymbolEvent }

const VERSION_FIELDS: Field[] = ['version', 'after', 'before']
const HEADER_HEIGHT = 56
const FILTER_BAR_PADDING = 2
const VERSION_ROW_HEIGHT = 52
const EVENT_ROW_HEIGHT = 58
const ROWS_BEFORE_END_TO_LOAD = 30
const AUTOCOMPLETE_DELAY_MILLISECONDS = 120
const SUGGESTION_LIMIT = 20

function useEvents(source: string, text: string) {
  return useInfiniteQuery({
    queryKey: ['events', source, text],
    queryFn: ({ pageParam }) => api.events(source, text, pageParam),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next,
    placeholderData: keepPreviousData,
  })
}

export function Search() {
  const source = useParams().source!
  const [parameters, setParameters] = useSearchParams()
  const text = parameters.get('q') ?? ''
  const events = useEvents(source, text)
  const error = events.isError ? events.error.message : null
  const query: Query = (error === null && events.data?.pages[0].query) || { and: [] }

  const summary = useSource(source)
  const versions = useQuery({
    queryKey: ['versions', source],
    queryFn: () => api.versions(source),
  })

  const { ref: filterBarRef, height: filterBarHeight } = useElementSize()
  const [versionSlot, setVersionSlot] = useState<HTMLElement | null>(null)
  const versioned = fields(query).some((field) => VERSION_FIELDS.includes(field))
  const onlyVersion = versioned
    ? null
    : (version: string) => setParameters({ q: withVersion(text, query, version) })

  useEffect(() => {
    document.title = text ? `${text} · ${source} · SourceDigger` : `${source} · SourceDigger`
  }, [text, source])

  return (
    <Stack gap="md" mt="md">
      <Group justify="space-between" align="center">
        <div>
          <Title order={2}>{source}</Title>
          {summary.data && (
            <Group gap={6} c="dimmed">
              <IconTag size={14} />
              <Text size="sm">
                {summary.data.version_count} versions
                {summary.data.first_version &&
                  ` · ${summary.data.first_version} – ${summary.data.last_version}`}
              </Text>
            </Group>
          )}
        </div>
        <Group gap="md">
          {summary.data && <DigState dig={summary.data.last_dig} />}
          <Button
            component={Link}
            to={settingsLink(source)}
            variant="light"
            leftSection={<IconSettings size={16} />}
          >
            Settings
          </Button>
        </Group>
      </Group>
      {versions.data && versions.data.length > 1 && (
        <ActivityStrip versions={versions.data} onSelect={onlyVersion} />
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
        <SearchBox
          key={text}
          source={source}
          value={text}
          error={error}
          versions={versions.data ?? []}
          onSearch={(next) => setParameters({ q: next })}
        />
        <Group gap="xs" mt="sm" mih={28} wrap="nowrap" align="center">
          <Box ref={setVersionSlot} ml="auto" visibleFrom="sm" />
        </Group>
      </Box>

      {error === null && (
        <Results
          source={source}
          events={events}
          query={query}
          onlyVersion={onlyVersion}
          stickyTop={HEADER_HEIGHT + filterBarHeight + FILTER_BAR_PADDING}
          versionSlot={versionSlot}
        />
      )}
    </Stack>
  )
}

function withVersion(text: string, query: Query, version: string): string {
  const narrowing = formatTerm(term('version', version))
  if (text === '') {
    return narrowing
  }
  if ('or' in query) {
    return `(${text}) ${narrowing}`
  }
  return `${text} ${narrowing}`
}

interface Completion {
  value: string
  field: Field | null
}

function lastTerm(input: string): { before: string; term: string } {
  const start = input.search(/-?\S*$/)
  const negation = input[start] === '-' ? 1 : 0
  return { before: input.slice(0, start + negation), term: input.slice(start + negation) }
}

function valueCompletions(field: Field, partial: string, versions: Version[]): string[] {
  if (field === 'kind' || field === 'action') {
    const chosen = partial.split(',')
    const typed = chosen.pop()!
    const all: string[] = field === 'kind' ? KINDS : ACTIONS
    return all
      .filter((choice) => choice.startsWith(typed) && !chosen.includes(choice))
      .map((choice) => [...chosen, choice].join(','))
  }
  if (field === 'version' || field === 'after' || field === 'before') {
    return versions
      .map((version) => version.name)
      .reverse()
      .filter((name) => name.startsWith(partial))
  }
  return []
}

function completions(input: string, names: string[], versions: Version[]): Completion[] {
  const { before, term } = lastTerm(input)
  const colon = term.indexOf(':')
  if (colon !== -1) {
    const field = term.slice(0, colon) as Field
    if (!Object.hasOwn(FIELDS, field)) {
      return []
    }
    return valueCompletions(field, term.slice(colon + 1), versions)
      .slice(0, SUGGESTION_LIMIT)
      .map((value) => ({ value: `${before}${field}:${value} `, field: null }))
  }
  const fields = (Object.keys(FIELDS) as Field[])
    .filter((field) => field !== 'name')
    .filter((field) => field.startsWith(term) && (term !== '' || before !== ''))
    .map((field) => ({ value: `${before}${field}:`, field }))
  const nameCompletions = names.map((name) => ({ value: `${before}${name} `, field: null }))
  return [...fields, ...nameCompletions]
}

interface SearchBoxProps {
  source: string
  value: string
  error: string | null
  versions: Version[]
  onSearch: (text: string) => void
}

function SearchBox({ source, value, error, versions, onSearch }: SearchBoxProps) {
  const [input, setInput] = useState(value)
  const [debounced] = useDebouncedValue(input, AUTOCOMPLETE_DELAY_MILLISECONDS)
  const inputRef = useRef<HTMLInputElement>(null)
  useHotkeys([['/', () => inputRef.current?.focus()]])

  const term = lastTerm(debounced).term
  const names = useQuery({
    queryKey: ['names', source, term],
    queryFn: () => api.names(source, term),
    enabled: term !== '' && !term.includes(':') && debounced !== value,
    placeholderData: keepPreviousData,
  })
  const options =
    input === value
      ? []
      : completions(input, term === '' || term.includes(':') ? [] : (names.data ?? []), versions)
  const fieldOf = new Map(options.map((option) => [option.value, option.field]))

  const run = (text: string) => onSearch(text.trim())
  const submit = (event: FormEvent) => {
    event.preventDefault()
    run(input)
  }
  const add = (example: string) => {
    const text = `${input.trim()} ${example}`.trim()
    setInput(text)
    run(text)
  }

  return (
    <form onSubmit={submit}>
      <Autocomplete
        ref={inputRef}
        size="md"
        value={input}
        onChange={setInput}
        onOptionSubmit={(option) => {
          setInput(option)
          if (fieldOf.get(option) === null) {
            run(option)
          }
        }}
        data={options.map((option) => option.value)}
        renderOption={({ option }) => (
          <CompletionOption
            text={lastTerm(option.value.trimEnd()).term}
            field={fieldOf.get(option.value) ?? null}
          />
        )}
        filter={({ options }) => options}
        limit={SUGGESTION_LIMIT * 2}
        error={input === value ? error : null}
        leftSection={<IconSearch size={18} />}
        rightSectionWidth={72}
        rightSection={
          <Group gap={6} wrap="nowrap">
            {names.isFetching ? (
              <Loader size="xs" />
            ) : (
              <Kbd size="xs" visibleFrom="sm">
                /
              </Kbd>
            )}
            <SyntaxHelp versions={versions} onExample={add} />
          </Group>
        }
        placeholder="Search"
        autoFocus
        spellCheck={false}
        autoCapitalize="off"
        autoComplete="off"
        comboboxProps={{ transitionProps: { duration: 0 } }}
      />
    </form>
  )
}

function CompletionOption({ text, field }: { text: string; field: Field | null }) {
  if (field === null) {
    return (
      <Text size="sm" ff="monospace">
        {text}
      </Text>
    )
  }
  const Icon = FIELD_ICONS[field]
  return (
    <Group gap="xs" wrap="nowrap">
      <Icon size={16} />
      <Text size="sm" ff="monospace">
        {text}
      </Text>
      <Text size="xs" c="dimmed">
        {FIELDS[field].meaning}
      </Text>
    </Group>
  )
}

function examples(versions: Version[]): Record<Field, string | null> {
  const newer = versions.at(-2)?.name
  const older = versions.at(-3)?.name
  return {
    name: FIELDS.name.example,
    kind: FIELDS.kind.example,
    action: FIELDS.action.example,
    version: newer === undefined ? null : `version:${newer}`,
    after: older === undefined ? null : `after:${older}`,
    before: newer === undefined ? null : `before:${newer}`,
    path: FIELDS.path.example,
    commit: null,
  }
}

interface SyntaxHelpProps {
  versions: Version[]
  onExample: (example: string) => void
}

function SyntaxHelp({ versions, onExample }: SyntaxHelpProps) {
  const [opened, setOpened] = useState(false)
  const clickable = examples(versions)
  const example = (text: string) => (
    <UnstyledButton
      key={text}
      onClick={() => {
        setOpened(false)
        onExample(text)
      }}
    >
      <Text span size="sm" ff="monospace" c="var(--mantine-primary-color-filled)">
        {text}
      </Text>
    </UnstyledButton>
  )
  return (
    <Popover opened={opened} onChange={setOpened} position="bottom-end" shadow="md" withArrow>
      <Popover.Target>
        <ActionIcon
          variant="subtle"
          color="gray"
          onClick={() => setOpened((current) => !current)}
          aria-label="Syntax"
        >
          <IconHelp size={18} />
        </ActionIcon>
      </Popover.Target>
      <Popover.Dropdown p="sm" maw="calc(100vw - 32px)">
        <Stack gap="sm">
          {(Object.keys(FIELDS) as Field[]).map((field) => {
            const text = clickable[field]
            return (
              <HelpRow key={field} icon={FIELD_ICONS[field]} meaning={FIELDS[field].meaning}>
                {text === null ? (
                  <Text span size="sm" ff="monospace" c="dimmed">
                    {FIELDS[field].example}
                  </Text>
                ) : (
                  example(text)
                )}
              </HelpRow>
            )
          })}
          <HelpRow icon={IconFilterOff} meaning="Leave out matches; repeated fields mean any">
            {example('-kind:define')}
          </HelpRow>
          <HelpRow icon={IconFilter} meaning="Combine with AND, OR, NOT and parentheses">
            {example('-(kind:function OR kind:define)')}
          </HelpRow>
        </Stack>
      </Popover.Dropdown>
    </Popover>
  )
}

interface ResultsProps {
  source: string
  events: ReturnType<typeof useEvents>
  query: Query
  onlyVersion: ((version: string) => void) | null
  stickyTop: number
  versionSlot: HTMLElement | null
}

function Results({ source, events, query, onlyVersion, stickyTop, versionSlot }: ResultsProps) {
  const rows = useMemo(() => {
    const rows: Row[] = []
    let version: string | null = null
    let commit: string | null = null
    for (const page of events.data?.pages ?? []) {
      for (const event of page.events) {
        if (event.version !== version) {
          version = event.version
          rows.push({ type: 'version', version })
        }
        if (event.commit.hash !== commit) {
          commit = event.commit.hash
          rows.push({ type: 'commit', commit: event.commit })
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

  if (events.isPending) {
    return (
      <Group justify="center" mt="xl">
        <Loader />
      </Group>
    )
  }
  if (rows.length === 0) {
    const name = 'term' in query && query.term.field === 'name' ? query.term.value : null
    return (
      <Stack align="center" mt="xl" gap={4}>
        <IconSearchOff size={32} color="var(--mantine-color-dimmed)" />
        <Text fw={500}>Nothing found</Text>
        {name !== null && !name.endsWith('*') && (
          <Anchor
            component={Link}
            size="sm"
            to={searchLink(source, formatTerm(term('name', `${name}*`)))}
          >
            {name}*
          </Anchor>
        )}
      </Stack>
    )
  }

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
        <Button
          component={Link}
          to={symbolLink(source, onlyName)}
          variant="light"
          fullWidth
          mb="sm"
          leftSection={<IconHistory size={16} />}
          rightSection={<IconArrowRight size={14} />}
        >
          Code and history of {onlyName}
        </Button>
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
                  {onlyVersion && (
                    <Button
                      variant="subtle"
                      size="compact-xs"
                      leftSection={<IconFilter size={12} />}
                      onClick={() => onlyVersion(row.version)}
                    >
                      Only
                    </Button>
                  )}
                </Group>
              ) : row.type === 'commit' ? (
                <CommitLine commit={row.commit} />
              ) : (
                <EventRow event={row.event} symbolLink={symbolLink(source, row.event.name)} />
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

interface HelpRowProps {
  icon: TablerIcon
  meaning: string
  children: ReactNode
}

function HelpRow({ icon: Icon, meaning, children }: HelpRowProps) {
  return (
    <Group gap="sm" wrap="nowrap" align="flex-start">
      <Icon size={16} style={{ flexShrink: 0, marginTop: 2 }} />
      <div>
        <Group gap="sm">{children}</Group>
        <Text size="xs" c="dimmed">
          {meaning}
        </Text>
      </div>
    </Group>
  )
}
