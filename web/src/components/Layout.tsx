import {
  ActionIcon,
  AppShell,
  Container,
  Group,
  Loader,
  Text,
  Tooltip,
  UnstyledButton,
  useComputedColorScheme,
  useMantineColorScheme,
} from '@mantine/core'
import { IconMoon, IconShovel, IconSun } from '@tabler/icons-react'
import { Suspense } from 'react'
import { Link, Outlet } from 'react-router'

import { QuickJump, QuickJumpButton } from './QuickJump'

export function Layout() {
  const { setColorScheme } = useMantineColorScheme()
  const colorScheme = useComputedColorScheme('light')
  const other = colorScheme === 'dark' ? 'light' : 'dark'

  return (
    <AppShell header={{ height: 56 }} padding="md">
      <QuickJump />
      <AppShell.Header>
        <Container size="lg" h="100%">
          <Group h="100%" justify="space-between">
            <UnstyledButton component={Link} to="/">
              <Group gap={8}>
                <IconShovel size={26} color="var(--mantine-color-orange-6)" />
                <Text fw={700} size="lg">
                  SourceDigger
                </Text>
              </Group>
            </UnstyledButton>
            <Group gap="sm" wrap="nowrap">
              <QuickJumpButton />
              <Tooltip label={`Switch to ${other} mode`}>
                <ActionIcon
                  variant="default"
                  size="lg"
                  onClick={() => setColorScheme(other)}
                  aria-label={`Switch to ${other} mode`}
                >
                  {colorScheme === 'dark' ? <IconSun size={18} /> : <IconMoon size={18} />}
                </ActionIcon>
              </Tooltip>
            </Group>
          </Group>
        </Container>
      </AppShell.Header>
      <AppShell.Main>
        <Container size="lg" pb="xl">
          <Suspense
            fallback={
              <Group justify="center" mt="xl">
                <Loader />
              </Group>
            }
          >
            <Outlet />
          </Suspense>
        </Container>
      </AppShell.Main>
    </AppShell>
  )
}
