import classes from './EventRow.module.css'
import { type Mark, renderTokens, tokenize } from './highlight'

interface SignatureProps {
  signature: string
  name: string
  symbolLink?: string
  marks?: Mark[]
}

export function Signature({ signature, name, symbolLink, marks }: SignatureProps) {
  return (
    <div className={classes.signature}>
      {renderTokens(tokenize(signature), { name, symbolLink, marks })}
    </div>
  )
}
