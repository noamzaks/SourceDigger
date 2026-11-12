import {
  Alert,
  Button,
  Card,
  Group,
  NumberInput,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core'
import {
  IconCpu,
  IconDatabase,
  IconDeviceFloppy,
  IconFileSettings,
  IconFolders,
  IconNetwork,
  IconWorldWww,
} from '@tabler/icons-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'

import { type Site, type SiteOptions, api } from '../api'

const FIELDS = [
  { key: 'database', label: 'Database', icon: IconDatabase },
  { key: 'repositories', label: 'Repositories', icon: IconFolders },
  { key: 'listen', label: 'Listen', icon: IconNetwork },
  { key: 'web_directory', label: 'Web', icon: IconWorldWww },
] as const

export function SiteSettingsPage() {
  const site = useQuery({ queryKey: ['site'], queryFn: api.site })
  return (
    <Stack gap="lg" mt="md">
      <Title order={2}>Settings</Title>
      {site.isError && <Alert color="red">{site.error.message}</Alert>}
      {site.data && <SiteForm site={site.data} />}
    </Stack>
  )
}

function SiteForm({ site }: { site: Site }) {
  const queryClient = useQueryClient()
  const [options, setOptions] = useState<SiteOptions>(site.saved)
  const save = useMutation({
    mutationFn: () => api.saveSite(options),
    onSuccess: (saved) => queryClient.setQueryData(['site'], saved),
  })
  const overridden = (key: keyof SiteOptions) => site.running.overridden[key] !== undefined
  const set = (key: keyof SiteOptions, value: string | number | undefined) =>
    setOptions({ ...options, [key]: value === '' ? undefined : value })

  return (
    <Card>
      <Stack>
        {FIELDS.map(({ key, label, icon: Icon }) => (
          <TextInput
            key={key}
            label={label}
            leftSection={<Icon size={16} />}
            description={overridden(key) ? 'Set at startup' : undefined}
            disabled={overridden(key)}
            placeholder={String(site.running[key] ?? '')}
            value={overridden(key) ? '' : (options[key] ?? '')}
            onChange={(event) => set(key, event.currentTarget.value)}
            styles={{ input: { fontFamily: 'var(--mantine-font-family-monospace)' } }}
          />
        ))}
        <NumberInput
          label="Threads"
          leftSection={<IconCpu size={16} />}
          description={overridden('threads') ? 'Set at startup' : undefined}
          disabled={overridden('threads')}
          placeholder={String(site.running.threads)}
          value={overridden('threads') ? '' : (options.threads ?? '')}
          onChange={(value) => set('threads', typeof value === 'number' ? value : undefined)}
          min={1}
          allowDecimal={false}
          w={200}
        />
        {save.isError && <Alert color="red">{save.error.message}</Alert>}
        <Group justify="space-between">
          <Group gap={6} c="dimmed" wrap="nowrap">
            <IconFileSettings size={16} style={{ flexShrink: 0 }} />
            <Text size="sm">{site.running.file} · applies on restart</Text>
          </Group>
          <Button
            onClick={() => save.mutate()}
            loading={save.isPending}
            leftSection={<IconDeviceFloppy size={16} />}
          >
            Save
          </Button>
        </Group>
      </Stack>
    </Card>
  )
}
