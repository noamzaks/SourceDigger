import { useQuery } from '@tanstack/react-query'
import { useParams, useSearchParams } from 'react-router'

import { type Action, type Revision, api } from '../api'

const MAXIMUM_FILES_TOGETHER = 25

export function revisionAction(revision: Revision, previous: Revision | undefined): Action {
  if (revision.definitions.length === 0) {
    return 'removed'
  }
  if (previous === undefined || previous.definitions.length === 0) {
    return 'added'
  }
  return 'modified'
}

export function useSymbol() {
  const source = useParams().source!
  const symbol = useParams().symbol!
  const [parameters, setParameters] = useSearchParams()
  const events = useQuery({
    queryKey: ['symbol', source, symbol],
    queryFn: () => api.symbol(source, symbol),
  })
  const paths = [
    ...new Set(
      events.data?.flatMap((event) => [
        event.definition.path,
        ...(event.previous ? [event.previous.path] : []),
      ]),
    ),
  ].sort()
  const tooManyPaths = paths.length > MAXIMUM_FILES_TOGETHER
  const path = parameters.get('path') ?? (tooManyPaths ? paths[0] : null)
  const revisions = useQuery({
    queryKey: ['revisions', source, symbol, path],
    queryFn: () => api.revisions(source, symbol, path),
    enabled: events.data !== undefined,
  })
  const setParameter = (key: 'path' | 'version', value: string | null) => {
    const next = new URLSearchParams(parameters)
    if (value === null) {
      next.delete(key)
    } else {
      next.set(key, value)
    }
    setParameters(next, { replace: true })
  }
  return {
    source,
    symbol,
    events,
    revisions,
    paths,
    path,
    tooManyPaths,
    version: parameters.get('version'),
    query: parameters.toString(),
    setParameter,
  }
}
