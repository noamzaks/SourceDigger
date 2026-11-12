const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
]

const relative = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })

export function ago(timestamp: number): string {
  const elapsed = timestamp - Date.now() / 1000
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(elapsed) >= seconds) {
      return relative.format(Math.round(elapsed / seconds), unit)
    }
  }
  return 'just now'
}

export function date(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString()
}

export function duration(seconds: number): string {
  const minutes = Math.floor(seconds / 60)
  return minutes > 0 ? `${minutes}m ${seconds % 60}s` : `${seconds}s`
}
