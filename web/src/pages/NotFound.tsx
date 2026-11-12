import { Button, Stack, Text, Title } from '@mantine/core'
import { Link } from 'react-router'

export function NotFound() {
  return (
    <Stack align="center" mt={80}>
      <Title order={2}>Nothing dug up here</Title>
      <Text c="dimmed">This page doesn't exist.</Text>
      <Button component={Link} to="/" variant="light">
        Back to all sources
      </Button>
    </Stack>
  )
}
