import { ActionIcon, Box, CopyButton, Text, Tooltip } from '@mantine/core'
import { IconCheck, IconCopy } from '@tabler/icons-react'

import { CodeBox } from './CodeBox'
import { renderTokens, splitLines, tokenize } from './highlight'

const LINE_NUMBER_STYLE = {
  userSelect: 'none',
  textAlign: 'right',
  paddingLeft: 'var(--mantine-spacing-sm)',
  paddingRight: 'var(--mantine-spacing-sm)',
  color: 'var(--mantine-color-dimmed)',
} as const

export function CodeBlock({
  code,
  name,
  firstLine,
}: {
  code: string
  name: string
  firstLine: number
}) {
  const lines = splitLines(tokenize(code))
  return (
    <Box pos="relative">
      <CopyButton value={code}>
        {({ copied, copy }) => (
          <Tooltip label={copied ? 'Copied' : 'Copy code'}>
            <ActionIcon
              variant="default"
              color={copied ? 'green' : 'gray'}
              onClick={copy}
              pos="absolute"
              top={8}
              right={8}
              aria-label="Copy code"
            >
              {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
            </ActionIcon>
          </Tooltip>
        )}
      </CopyButton>
      <CodeBox>
        <table style={{ borderCollapse: 'collapse' }}>
          <tbody>
            {lines.map((tokens, index) => (
              <tr key={index}>
                <td style={LINE_NUMBER_STYLE}>{firstLine + index}</td>
                <td>
                  <Text inherit span>
                    {renderTokens(tokens, { name })}
                  </Text>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CodeBox>
    </Box>
  )
}
