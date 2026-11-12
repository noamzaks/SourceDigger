import { IconArrowRight, IconExternalLink } from '@tabler/icons-react'
import { diffWords } from 'diff'
import { memo } from 'react'

import type { Action, Kind, Place, SymbolEvent } from '../api'
import classes from './EventRow.module.css'
import type { Mark } from './highlight'
import { Signature } from './Signature'
import { ACTION_ICONS, KIND_ICONS } from './symbolStyle'

export function ActionIcon({ action }: { action: Action }) {
  const Icon = ACTION_ICONS[action]
  return (
    <span className={classes.action} data-action={action} title={action}>
      <Icon size={14} />
    </span>
  )
}

export function KindBadge({ kind }: { kind: Kind }) {
  const Icon = KIND_ICONS[kind]
  return (
    <span className={classes.kind}>
      <Icon size={12} />
      {kind}
    </span>
  )
}

export function Location({ place }: { place: Place }) {
  const text = place.path
    .split('/')
    .flatMap((part, index) => (index === 0 ? [part] : ['/', <wbr key={index} />, part]))
    .concat(`:${place.line}`)
  if (!place.url) {
    return <span className={classes.location}>{text}</span>
  }
  return (
    <a className={classes.location} href={place.url} target="_blank" rel="noreferrer">
      {text} <IconExternalLink size={12} style={{ verticalAlign: 'middle' }} />
    </a>
  )
}

function changeMarks(before: string, after: string): { removed: Mark[]; added: Mark[] } {
  const removed: Mark[] = []
  const added: Mark[] = []
  let beforeOffset = 0
  let afterOffset = 0
  for (const part of diffWords(before, after)) {
    if (part.removed) {
      removed.push({
        start: beforeOffset,
        end: beforeOffset + part.value.length,
        change: 'removed',
      })
      beforeOffset += part.value.length
    } else if (part.added) {
      added.push({ start: afterOffset, end: afterOffset + part.value.length, change: 'added' })
      afterOffset += part.value.length
    } else {
      beforeOffset += part.value.length
      afterOffset += part.value.length
    }
  }
  return { removed, added }
}

export function Change({ event, symbolLink }: { event: SymbolEvent; symbolLink?: string }) {
  const previous = event.previous
  if (previous === null) {
    return (
      <Signature signature={event.definition.signature} name={event.name} symbolLink={symbolLink} />
    )
  }
  const marks = changeMarks(previous.signature, event.definition.signature)
  return (
    <>
      <Signature
        signature={event.definition.signature}
        name={event.name}
        symbolLink={symbolLink}
        marks={marks.added}
      />
      {previous.signature !== event.definition.signature && (
        <div className={classes.note}>
          was
          <Signature signature={previous.signature} name={event.name} marks={marks.removed} />
        </div>
      )}
      {previous.path !== event.definition.path && (
        <div className={classes.note}>
          moved from <Location place={previous} />
          <IconArrowRight size={12} />
          <Location place={event.definition} />
        </div>
      )}
    </>
  )
}

interface EventRowProps {
  event: SymbolEvent
  symbolLink: string
}

export const EventRow = memo(function EventRow({ event, symbolLink }: EventRowProps) {
  return (
    <div className={classes.row}>
      <ActionIcon action={event.action} />
      <div className={classes.body}>
        <Change event={event} symbolLink={symbolLink} />
        <div className={classes.inlineMeta}>
          <KindBadge kind={event.kind} />
          <Location place={event.definition} />
        </div>
      </div>
      <div className={classes.meta}>
        <KindBadge kind={event.kind} />
        <Location place={event.definition} />
      </div>
    </div>
  )
})
