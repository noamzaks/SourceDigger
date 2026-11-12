import { Box, Group, Text, Tooltip } from '@mantine/core'
import { type MouseEvent, useState } from 'react'

import type { Version } from '../api'

const HEIGHT = 40
const MINIMUM_BAR_HEIGHT = 2
const BAR_WIDTH = 10
const GAP_WIDTH = 2

interface ActivityStripProps {
  versions: Version[]
  onSelect: ((version: string) => void) | null
}

function changes(version: Version): number {
  return version.added + version.removed + version.modified + version.signature
}

export function ActivityStrip({ versions, onSelect }: ActivityStripProps) {
  const [hovered, setHovered] = useState<number | null>(null)
  const largest = Math.log1p(Math.max(...versions.map(changes)))
  const step = BAR_WIDTH + GAP_WIDTH
  const width = versions.length * step

  const indexAt = (event: MouseEvent<SVGSVGElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect()
    const index = Math.floor(((event.clientX - bounds.left) / bounds.width) * versions.length)
    return Math.min(versions.length - 1, Math.max(0, index))
  }
  const version = hovered === null ? null : versions[hovered]

  return (
    <Box>
      <Tooltip.Floating
        disabled={version === null}
        label={
          version && (
            <>
              <Text size="sm" fw={600}>
                {version.name}
              </Text>
              <Text size="xs">
                {version.added.toLocaleString()} added · {version.removed.toLocaleString()} removed
                · {version.modified.toLocaleString()} modified ·{' '}
                {version.signature.toLocaleString()} signature
              </Text>
            </>
          )
        }
      >
        <svg
          viewBox={`0 0 ${width} ${HEIGHT}`}
          preserveAspectRatio="none"
          width="100%"
          height={HEIGHT}
          role="img"
          aria-label="Changes per version"
          style={{ display: 'block', cursor: onSelect ? 'pointer' : 'default' }}
          onMouseMove={(event) => setHovered(indexAt(event))}
          onMouseLeave={() => setHovered(null)}
          onClick={(event) => onSelect?.(versions[indexAt(event)].name)}
        >
          {versions.map((version, index) => {
            const height = Math.max(
              MINIMUM_BAR_HEIGHT,
              (HEIGHT * Math.log1p(changes(version))) / largest,
            )
            return (
              <rect
                key={version.name}
                x={index * step}
                y={HEIGHT - height}
                width={BAR_WIDTH}
                height={height}
                rx={2}
                fill={
                  index === hovered
                    ? 'var(--mantine-color-orange-8)'
                    : 'var(--mantine-color-orange-filled)'
                }
              />
            )
          })}
        </svg>
      </Tooltip.Floating>
      <Group justify="space-between" mt={4}>
        <Text size="xs" c="dimmed">
          {versions[0].name}
        </Text>
        <Text size="xs" c="dimmed">
          {versions.at(-1)!.name}
        </Text>
      </Group>
    </Box>
  )
}
