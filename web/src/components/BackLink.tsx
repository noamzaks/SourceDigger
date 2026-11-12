import { Anchor, Group } from '@mantine/core'
import { IconArrowLeft } from '@tabler/icons-react'
import { Link } from 'react-router'

export function BackLink({ to, label }: { to: string; label: string }) {
  return (
    <Anchor component={Link} to={to} size="sm">
      <Group gap={4} component="span">
        <IconArrowLeft size={14} />
        {label}
      </Group>
    </Anchor>
  )
}
