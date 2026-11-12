import {
  type Icon,
  IconHash,
  IconMathFunction,
  IconMinus,
  IconPencil,
  IconPlus,
  IconVariable,
} from '@tabler/icons-react'

import type { Action, Kind } from '../api'

export const ACTION_COLORS: Record<Action, string> = {
  added: 'green',
  removed: 'red',
  modified: 'yellow',
}

export const ACTION_ICONS: Record<Action, Icon> = {
  added: IconPlus,
  removed: IconMinus,
  modified: IconPencil,
}

export const KIND_ICONS: Record<Kind, Icon> = {
  function: IconMathFunction,
  define: IconHash,
  variable: IconVariable,
}
