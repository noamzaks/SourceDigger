import { IconGitCommit } from '@tabler/icons-react'
import { Link, useParams } from 'react-router'

import type { Commit } from '../api'
import { formatTerm, term } from '../searchQuery'
import { searchLink } from '../links'
import { ago, date } from '../time'
import classes from './EventRow.module.css'

const SHORT_HASH_LENGTH = 12

export function CommitLine({ commit }: { commit: Commit }) {
  const source = useParams().source!
  const hash = commit.hash.slice(0, SHORT_HASH_LENGTH)
  const link = searchLink(source, formatTerm(term('commit', hash)))
  return (
    <div className={classes.commit}>
      <IconGitCommit size={14} />
      <Link to={link} className={classes.commitLink} title={commit.hash}>
        <span className={classes.hash}>{hash}</span>
        <span className={classes.summary}>{commit.summary}</span>
      </Link>
      <span className={classes.author} title={date(commit.time)}>
        {commit.author} · {ago(commit.time)}
      </span>
    </div>
  )
}
