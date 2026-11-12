import { Box } from '@mantine/core'
import type { ReactNode } from 'react'

export function CodeBox({ children }: { children: ReactNode }) {
  return (
    <Box
      component="pre"
      m={0}
      py="xs"
      fz="sm"
      ff="monospace"
      bg="var(--mantine-color-default-hover)"
      style={{ overflowX: 'auto', borderRadius: 'var(--mantine-radius-md)', tabSize: 4 }}
    >
      {children}
    </Box>
  )
}
