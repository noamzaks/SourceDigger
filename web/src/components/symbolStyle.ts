import {
  type Icon,
  IconArrowBarLeft,
  IconArrowBarRight,
  IconBolt,
  IconBraces,
  IconBrackets,
  IconList,
  IconListNumbers,
  IconCategory,
  IconEqual,
  IconFolder,
  IconGitCommit,
  IconHash,
  IconMathFunction,
  IconMinus,
  IconPencil,
  IconPlus,
  IconPlusMinus,
  IconRegex,
  IconSignature,
  IconTag,
  IconVariable,
} from '@tabler/icons-react'

import type { Action, Field, Kind } from '../api'

export const ACTION_COLORS: Record<Action, string> = {
  added: 'green',
  removed: 'red',
  modified: 'yellow',
  signature: 'orange',
}

export const ACTION_TITLES: Record<Action, string> = {
  added: 'Added in',
  removed: 'Removed in',
  modified: 'Changed in',
  signature: 'Signature changed in',
}

export const ACTION_ICONS: Record<Action, Icon> = {
  added: IconPlus,
  removed: IconMinus,
  modified: IconPencil,
  signature: IconSignature,
}

export const KIND_ICONS: Record<Kind, Icon> = {
  function: IconMathFunction,
  define: IconHash,
  variable: IconVariable,
  struct: IconBraces,
  union: IconBrackets,
  enum: IconListNumbers,
  enumerator: IconList,
  typedef: IconEqual,
  invocation: IconBolt,
}

export const FIELD_ICONS: Record<Field, Icon> = {
  name: IconRegex,
  kind: IconCategory,
  action: IconPlusMinus,
  version: IconTag,
  after: IconArrowBarRight,
  before: IconArrowBarLeft,
  path: IconFolder,
  commit: IconGitCommit,
}
