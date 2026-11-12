import {
  Accordion,
  Alert,
  Badge,
  Button,
  Card,
  Code,
  Group,
  Menu,
  Modal,
  NumberInput,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import { useForm } from '@mantine/form'
import { notifications } from '@mantine/notifications'
import {
  IconBrandGit,
  IconBulb,
  IconClockPlay,
  IconEye,
  IconExternalLink,
  IconFileCode,
  IconId,
  IconRefresh,
  IconRestore,
  IconTags,
  IconTrash,
} from '@tabler/icons-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router'

import { type Dig, type DigStatus, type SourceSettings, api, isDigging } from '../api'
import { BackLink } from '../components/BackLink'
import { DIGGING_REFRESH_MILLISECONDS, useSource } from '../queries'
import { ago, date, duration } from '../time'

const DIG_STATUS_COLORS: Record<DigStatus, string> = {
  queued: 'gray',
  running: 'blue',
  succeeded: 'green',
  failed: 'red',
}
const PREVIEW_SHOWN_VERSIONS = 60
const SECONDS_PER_HOUR = 3600

const NEW_SOURCE: SourceSettings = {
  name: '',
  url: '',
  tag_pattern: String.raw`^v(\d+(?:\.\d+)*)$`,
  file_pattern: String.raw`\.[ch]$`,
  viewer_url: '',
  refresh_hours: null,
}

const EXAMPLES: SourceSettings[] = [
  {
    name: 'linux',
    url: 'https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git',
    tag_pattern: String.raw`^v(\d+\.\d+(?:\.\d+)?)$`,
    file_pattern: String.raw`\.[ch]$`,
    viewer_url: 'https://elixir.bootlin.com/linux/{tag}/source/{path}#L{line}',
    refresh_hours: 24,
  },
  {
    name: 'musl',
    url: 'https://git.musl-libc.org/git/musl',
    tag_pattern: String.raw`^v(\d+\.\d+\.\d+)$`,
    file_pattern: String.raw`\.[ch]$`,
    viewer_url: 'https://git.musl-libc.org/cgit/musl/tree/{path}?h={tag}#n{line}',
    refresh_hours: 24,
  },
  {
    name: 'git',
    url: 'https://github.com/git/git.git',
    tag_pattern: String.raw`^v(\d+\.\d+)\.0$`,
    file_pattern: String.raw`\.[ch]$`,
    viewer_url: 'https://github.com/git/git/blob/{tag}/{path}#L{line}',
    refresh_hours: 24,
  },
  {
    name: 'glibc',
    url: 'https://sourceware.org/git/glibc.git',
    tag_pattern: String.raw`^glibc-(\d+\.\d+)$`,
    file_pattern: String.raw`\.[ch]$`,
    viewer_url: 'https://sourceware.org/git/?p=glibc.git;a=blob;f={path};hb={tag}#l{line}',
    refresh_hours: 24,
  },
  {
    name: 'vim',
    url: 'https://github.com/vim/vim.git',
    tag_pattern: String.raw`^v(\d+\.\d)(?:\.0000)?$`,
    file_pattern: String.raw`\.[ch]$`,
    viewer_url: 'https://github.com/vim/vim/blob/{tag}/{path}#L{line}',
    refresh_hours: 24,
  },
]

function githubViewer(url: string): string | null {
  const match = url.match(/^https:\/\/github\.com\/([^/]+\/[^/]+?)(?:\.git)?\/?$/)
  return match ? `https://github.com/${match[1]}/blob/{tag}/{path}#L{line}` : null
}

export function SourceSettingsPage() {
  const [template, setTemplate] = useState(NEW_SOURCE)
  const source = useParams().source
  if (source === undefined) {
    return (
      <Stack gap="lg" mt="md">
        <Group justify="space-between">
          <Title order={2}>Add source</Title>
          <Menu position="bottom-end">
            <Menu.Target>
              <Button variant="default" leftSection={<IconBulb size={16} />}>
                Start from an example
              </Button>
            </Menu.Target>
            <Menu.Dropdown>
              {EXAMPLES.map((example) => (
                <Menu.Item key={example.name} onClick={() => setTemplate(example)}>
                  {example.name}
                </Menu.Item>
              ))}
            </Menu.Dropdown>
          </Menu>
        </Group>
        <SettingsForm key={template.name} initial={template} />
      </Stack>
    )
  }
  return (
    <Stack gap="lg" mt="md">
      <Group justify="space-between" align="flex-end">
        <div>
          <BackLink to={`/sources/${encodeURIComponent(source)}`} label={`Search ${source}`} />
          <Title order={2}>{source} settings</Title>
        </div>
      </Group>
      <ExistingSource source={source} />
    </Stack>
  )
}

function ExistingSource({ source }: { source: string }) {
  const summary = useSource(source)
  if (summary.isError) {
    return <Alert color="red">{summary.error.message}</Alert>
  }
  if (summary.data === undefined) {
    return null
  }
  return (
    <>
      <SettingsForm key={summary.data.id} initial={summary.data} existing={source} />
      <Digs source={source} refreshHours={summary.data.refresh_hours} />
      <DangerZone source={source} />
    </>
  )
}

function SettingsForm({ initial, existing }: { initial: SourceSettings; existing?: string }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const form = useForm<SourceSettings>({
    initialValues: {
      name: initial.name,
      url: initial.url,
      tag_pattern: initial.tag_pattern,
      file_pattern: initial.file_pattern,
      viewer_url: initial.viewer_url,
      refresh_hours: initial.refresh_hours,
    },
    onValuesChange: (values, previous) => {
      const viewer = githubViewer(values.url)
      if (values.url !== previous.url && viewer && !values.viewer_url) {
        form.setFieldValue('viewer_url', viewer)
      }
    },
    validate: {
      name: (value) =>
        /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value) ? null : 'Letters, digits, ".", "_" or "-"',
      url: (value) => (value.trim() ? null : 'Required'),
    },
  })

  const preview = useMutation({
    mutationFn: () => api.preview(form.values.url, form.values.tag_pattern),
  })

  const save = useMutation({
    mutationFn: async (settings: SourceSettings) => {
      if (existing === undefined) {
        await api.createSource(settings)
        await api.startDig(settings.name)
      } else {
        await api.updateSource(existing, settings)
      }
      return settings
    },
    onSuccess: async (settings) => {
      await queryClient.invalidateQueries()
      notifications.show({
        color: 'green',
        message: existing === undefined ? `Digging ${settings.name}…` : 'Saved',
      })
      navigate(`/sources/${encodeURIComponent(settings.name)}/settings`, { replace: true })
    },
  })

  const changesFiles = existing !== undefined && form.values.file_pattern !== initial.file_pattern

  return (
    <Card withBorder p={{ base: 'md', sm: 'lg' }}>
      <form onSubmit={form.onSubmit((settings) => save.mutate(settings))}>
        <Stack>
          <TextInput
            label="Name"
            leftSection={<IconId size={16} />}
            description="Shown on the home page and used in links"
            placeholder="linux"
            withAsterisk
            {...form.getInputProps('name')}
          />
          <TextInput
            label="Repository"
            leftSection={<IconBrandGit size={16} />}
            description="A git URL or a path on this machine"
            placeholder="https://github.com/torvalds/linux.git"
            withAsterisk
            {...form.getInputProps('url')}
          />
          <Group align="flex-end" wrap="nowrap">
            <TextInput
              label="Tag pattern"
              leftSection={<IconTags size={16} />}
              description="A regular expression choosing which tags are versions; its first group, if any, names the version. Releases from one branch work best: maintenance tags of older branches make symbols seem to disappear and come back."
              style={{ flex: 1 }}
              styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
              {...form.getInputProps('tag_pattern')}
            />
            <Button
              variant="default"
              leftSection={<IconEye size={16} />}
              onClick={() => preview.mutate()}
              loading={preview.isPending}
            >
              Preview
            </Button>
          </Group>
          {preview.isError && <Alert color="red">{preview.error.message}</Alert>}
          {preview.data && (
            <Alert
              color={preview.data.length === 0 ? 'yellow' : 'blue'}
              title={`${preview.data.length} versions match`}
            >
              <Group gap={4}>
                {preview.data.slice(0, PREVIEW_SHOWN_VERSIONS).map((version) => (
                  <Badge key={version.tag} variant="default" radius="sm" tt="none">
                    {version.name}
                  </Badge>
                ))}
                {preview.data.length > PREVIEW_SHOWN_VERSIONS && (
                  <Text size="sm" c="dimmed">
                    …and {preview.data.length - PREVIEW_SHOWN_VERSIONS} more, up to{' '}
                    {preview.data.at(-1)!.name}
                  </Text>
                )}
              </Group>
            </Alert>
          )}
          <TextInput
            label="File pattern"
            leftSection={<IconFileCode size={16} />}
            description="A regular expression over file paths choosing which files to index"
            styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
            {...form.getInputProps('file_pattern')}
          />
          {changesFiles && (
            <Text size="sm" c="yellow">
              Changing the file pattern discards every dug version; the next dig starts over.
            </Text>
          )}
          <TextInput
            label="Source viewer"
            leftSection={<IconExternalLink size={16} />}
            description="Where symbols link to, with {tag}, {path} and {line} filled in"
            placeholder="https://github.com/torvalds/linux/blob/{tag}/{path}#L{line}"
            styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
            {...form.getInputProps('viewer_url')}
          />
          <NumberInput
            label="Dig automatically every"
            leftSection={<IconClockPlay size={16} />}
            description="Leave empty to dig only when asked"
            suffix=" hours"
            min={1}
            allowDecimal={false}
            w={260}
            {...form.getInputProps('refresh_hours')}
            onChange={(value) =>
              form.setFieldValue('refresh_hours', typeof value === 'number' ? value : null)
            }
          />
          {save.isError && <Alert color="red">{save.error.message}</Alert>}
          <Group justify="flex-end">
            <Button type="submit" loading={save.isPending}>
              {existing === undefined ? 'Add and dig' : 'Save'}
            </Button>
          </Group>
        </Stack>
      </form>
    </Card>
  )
}

function Digs({ source, refreshHours }: { source: string; refreshHours: number | null }) {
  const queryClient = useQueryClient()
  const digs = useQuery({
    queryKey: ['digs', source],
    queryFn: () => api.digs(source),
    refetchInterval: (query) =>
      query.state.data?.some(isDigging) ? DIGGING_REFRESH_MILLISECONDS : false,
  })
  const digging = digs.data?.some(isDigging) ?? false

  const start = useMutation({
    mutationFn: () => api.startDig(source),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['digs', source] })
      queryClient.invalidateQueries({ queryKey: ['source', source] })
    },
  })

  return (
    <Card withBorder p={{ base: 'md', sm: 'lg' }}>
      <Group justify="space-between" mb="md">
        <Title order={3}>Digs</Title>
        <Button
          variant="light"
          leftSection={<IconRefresh size={16} />}
          onClick={() => start.mutate()}
          loading={start.isPending}
          disabled={digging}
        >
          Dig now
        </Button>
      </Group>
      {start.isError && <Alert color="red">{start.error.message}</Alert>}
      {refreshHours !== null && digs.data?.[0] && !digging && (
        <Group gap={6} mb="sm">
          <IconClockPlay size={16} color="var(--mantine-color-dimmed)" />
          <Text size="sm" c="dimmed">
            Next automatic dig {ago(digs.data[0].queued_at + refreshHours * SECONDS_PER_HOUR)}
          </Text>
        </Group>
      )}
      {digs.data?.length === 0 && <Text c="dimmed">Never dug.</Text>}
      {digs.data && digs.data.length > 0 && (
        <Accordion variant="separated" defaultValue={String(digs.data[0].id)}>
          {digs.data.map((dig) => (
            <DigItem key={dig.id} dig={dig} />
          ))}
        </Accordion>
      )}
    </Card>
  )
}

function DigItem({ dig }: { dig: Dig }) {
  const finished =
    dig.finished_at && dig.started_at ? duration(dig.finished_at - dig.started_at) : null
  return (
    <Accordion.Item value={String(dig.id)}>
      <Accordion.Control>
        <Group justify="space-between" pr="sm">
          <Group gap="sm">
            <Badge color={DIG_STATUS_COLORS[dig.status]} variant="light">
              {dig.status}
            </Badge>
            <Text size="sm" title={date(dig.queued_at)}>
              {date(dig.started_at ?? dig.queued_at)}
            </Text>
          </Group>
          <Text size="xs" c="dimmed">
            {dig.versions_done} / {dig.versions_total} versions
            {finished && ` · took ${finished}`}
          </Text>
        </Group>
      </Accordion.Control>
      <Accordion.Panel>
        {dig.error && (
          <Alert color="red" mb="sm">
            {dig.error}
          </Alert>
        )}
        <ScrollArea.Autosize mah={300}>
          <Code block>{dig.log || 'Fetching…'}</Code>
        </ScrollArea.Autosize>
      </Accordion.Panel>
    </Accordion.Item>
  )
}

function DangerZone({ source }: { source: string }) {
  const [opened, setOpened] = useState(false)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const remove = useMutation({
    mutationFn: () => api.deleteSource(source),
    onSuccess: async () => {
      await queryClient.invalidateQueries()
      navigate('/')
    },
  })
  const redig = useMutation({
    mutationFn: () => api.startDig(source, 'scratch'),
    onSuccess: () => queryClient.invalidateQueries(),
  })

  return (
    <Card withBorder p={{ base: 'md', sm: 'lg' }}>
      <Group justify="space-between" mb="md">
        <div>
          <Text fw={600}>Dig again from scratch</Text>
          <Text size="sm" c="dimmed">
            Forgets every dug version and digs them all again, for example after SourceDigger
            learned to read code better.
          </Text>
        </div>
        <Button
          color="red"
          variant="light"
          leftSection={<IconRestore size={16} />}
          onClick={() => redig.mutate()}
          loading={redig.isPending}
        >
          Dig again
        </Button>
      </Group>
      {redig.isError && (
        <Alert color="red" mb="md">
          {redig.error.message}
        </Alert>
      )}
      <Group justify="space-between">
        <div>
          <Text fw={600}>Delete this source</Text>
          <Text size="sm" c="dimmed">
            Removes its versions, symbols, digs and the fetched repository.
          </Text>
        </div>
        <Button
          color="red"
          variant="light"
          leftSection={<IconTrash size={16} />}
          onClick={() => setOpened(true)}
        >
          Delete
        </Button>
      </Group>
      <Modal opened={opened} onClose={() => setOpened(false)} title={`Delete ${source}?`} centered>
        <Text size="sm">
          Everything dug for {source} will be gone; digging it again starts from scratch.
        </Text>
        {remove.isError && (
          <Alert color="red" mt="sm">
            {remove.error.message}
          </Alert>
        )}
        <Group justify="flex-end" mt="lg">
          <Button variant="default" onClick={() => setOpened(false)}>
            Cancel
          </Button>
          <Button color="red" onClick={() => remove.mutate()} loading={remove.isPending}>
            Delete {source}
          </Button>
        </Group>
      </Modal>
    </Card>
  )
}
