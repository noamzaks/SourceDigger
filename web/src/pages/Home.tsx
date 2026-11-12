import {
  ActionIcon,
  Alert,
  Button,
  Card,
  Group,
  SimpleGrid,
  Skeleton,
  Stack,
  Text,
  Title,
  Tooltip,
} from '@mantine/core'
import { notifications } from '@mantine/notifications'
import {
  IconBrandGit,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconSettings,
  IconShovel,
  IconTags,
} from '@tabler/icons-react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router'

import { type SourceSummary, api, isDigging } from '../api'
import { DigState } from '../components/DigState'
import { settingsLink, sourceLink } from '../links'
import { useSources } from '../queries'

export function Home() {
  const sources = useSources()

  return (
    <Stack gap="lg" mt="md">
      <Group justify="space-between" align="center">
        <Title order={2}>Sources</Title>
        <Button component={Link} to="/add" leftSection={<IconPlus size={16} />}>
          Add
        </Button>
      </Group>
      {sources.isError && <Alert color="red">{sources.error.message}</Alert>}
      {sources.isPending && (
        <SimpleGrid cols={{ base: 1, sm: 2, md: 3 }}>
          {[0, 1, 2].map((index) => (
            <Skeleton key={index} h={150} />
          ))}
        </SimpleGrid>
      )}
      {sources.data?.length === 0 && <EmptyState />}
      <SimpleGrid cols={{ base: 1, sm: 2, md: 3 }}>
        {sources.data?.map((source) => (
          <SourceCard key={source.id} source={source} />
        ))}
      </SimpleGrid>
    </Stack>
  )
}

function EmptyState() {
  return (
    <Card p="xl">
      <Stack align="center" gap="xs">
        <IconShovel size={40} color="var(--mantine-color-orange-6)" />
        <Title order={3}>No sources</Title>
        <Button component={Link} to="/add" mt="sm" leftSection={<IconPlus size={16} />}>
          Add
        </Button>
      </Stack>
    </Card>
  )
}

function SourceCard({ source }: { source: SourceSummary }) {
  const queryClient = useQueryClient()
  const dig = useMutation({
    mutationFn: () => api.startDig(source.name),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['sources'] }),
    onError: (error) =>
      notifications.show({ color: 'red', title: 'Dig failed to start', message: error.message }),
  })

  return (
    <Card>
      <Stack gap="sm" h="100%" justify="space-between">
        <div>
          <Group justify="space-between" wrap="nowrap">
            <Text
              component={Link}
              to={sourceLink(source.name)}
              fw={700}
              size="xl"
              c="var(--mantine-color-text)"
              td="none"
              truncate
            >
              {source.name}
            </Text>
            <Group gap={4} wrap="nowrap">
              <Tooltip label="Dig">
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  onClick={() => dig.mutate()}
                  loading={dig.isPending}
                  disabled={isDigging(source.last_dig)}
                  aria-label="Dig"
                >
                  <IconRefresh size={18} />
                </ActionIcon>
              </Tooltip>
              <Tooltip label="Settings">
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  component={Link}
                  to={settingsLink(source.name)}
                  aria-label="Settings"
                >
                  <IconSettings size={18} />
                </ActionIcon>
              </Tooltip>
            </Group>
          </Group>
          <Group gap={4} wrap="nowrap">
            <IconBrandGit size={12} color="var(--mantine-color-dimmed)" style={{ flexShrink: 0 }} />
            <Text size="xs" c="dimmed" truncate>
              {source.url}
            </Text>
          </Group>
        </div>
        <Group gap={6} wrap="nowrap">
          <IconTags size={16} color="var(--mantine-color-dimmed)" />
          <Text size="sm">
            {source.version_count === 0
              ? 'No versions'
              : `${source.version_count} versions · ${source.first_version} – ${source.last_version}`}
          </Text>
        </Group>
        <DigState dig={source.last_dig} />
        <Button
          component={Link}
          to={sourceLink(source.name)}
          variant="light"
          disabled={source.version_count === 0}
          leftSection={<IconSearch size={16} />}
        >
          Search
        </Button>
      </Stack>
    </Card>
  )
}
