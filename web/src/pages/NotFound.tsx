import { Button, Stack, Title } from '@mantine/core'
import { Link } from 'react-router'

export function NotFound() {
  return (
    <Stack align="center" mt={80}>
      <Title order={2}>Not found</Title>
      <Button component={Link} to="/" variant="light">
        Home
      </Button>
    </Stack>
  )
}
