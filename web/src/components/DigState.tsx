import { Badge, Group, Progress, Stack, Text, Tooltip } from '@mantine/core'
import { IconAlertTriangle, IconCircleCheck, IconHourglass } from '@tabler/icons-react'

import type { Dig } from '../api'
import { ago, date } from '../time'

export function DigState({ dig }: { dig: Dig | null }) {
  if (dig === null) {
    return (
      <Text size="sm" c="dimmed">
        Never dug
      </Text>
    )
  }
  switch (dig.status) {
    case 'queued':
      return (
        <Badge variant="light" color="gray" leftSection={<IconHourglass size={12} />}>
          Waiting to dig
        </Badge>
      )
    case 'running': {
      const fetching = dig.versions_total === 0
      const [done, total, unit] = fetching
        ? [dig.objects_received, dig.objects_total, 'objects fetched']
        : [dig.versions_done, dig.versions_total, 'versions']
      return (
        <Stack gap={4}>
          <Group justify="space-between" wrap="nowrap">
            <Text size="sm" fw={500}>
              {fetching ? 'Fetching…' : 'Digging…'}
            </Text>
            <Text size="xs" c="dimmed">
              {total > 0 && `${done.toLocaleString()} / ${total.toLocaleString()} ${unit}`}
            </Text>
          </Group>
          <Progress
            value={total === 0 ? 100 : (100 * done) / total}
            animated
            striped={total === 0}
            size="sm"
          />
        </Stack>
      )
    }
    case 'succeeded':
      return (
        <Tooltip label={date(dig.finished_at!)}>
          <Group gap={6} wrap="nowrap" w="fit-content">
            <IconCircleCheck size={16} color="var(--mantine-color-green-6)" />
            <Text size="sm" c="dimmed">
              Dug {ago(dig.finished_at!)}
            </Text>
          </Group>
        </Tooltip>
      )
    case 'failed':
      return (
        <Tooltip label={dig.error} multiline maw={400}>
          <Group gap={6} wrap="nowrap" w="fit-content">
            <IconAlertTriangle size={16} color="var(--mantine-color-red-6)" />
            <Text size="sm" c="red">
              Dig failed {ago(dig.finished_at!)}
            </Text>
          </Group>
        </Tooltip>
      )
  }
}
