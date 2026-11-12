import { queryOptions, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'

import { api, isDigging } from './api'

export const DIGGING_REFRESH_MILLISECONDS = 1000

export const sourcesQuery = queryOptions({ queryKey: ['sources'], queryFn: api.sources })

export function useSources() {
  return useQuery({
    ...sourcesQuery,
    refetchInterval: (query) =>
      query.state.data?.some((source) => isDigging(source.last_dig))
        ? DIGGING_REFRESH_MILLISECONDS
        : false,
  })
}

export function useSource(source: string) {
  const queryClient = useQueryClient()
  const summary = useQuery({
    queryKey: ['source', source],
    queryFn: () => api.source(source),
    refetchInterval: (query) =>
      isDigging(query.state.data?.last_dig ?? null) ? DIGGING_REFRESH_MILLISECONDS : false,
  })

  const versionCount = summary.data?.version_count
  const seenVersionCount = useRef(versionCount)
  useEffect(() => {
    if (seenVersionCount.current !== undefined && seenVersionCount.current !== versionCount) {
      for (const key of ['events', 'versions', 'names', 'symbol', 'revisions']) {
        queryClient.invalidateQueries({ queryKey: [key, source] })
      }
    }
    seenVersionCount.current = versionCount
  }, [queryClient, source, versionCount])

  return summary
}
