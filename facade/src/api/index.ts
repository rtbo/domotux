const envHost = import.meta.env.VITE_API_HOST
const envPort = import.meta.env.VITE_API_PORT || 3030
const useTls = import.meta.env.VITE_API_USE_TLS === 'true'

export function makeUrl (protocol: string, path: string): string {
  const host = envHost ?? window.location.hostname
  const port = (envPort === 80 && protocol === 'http') || (envPort === 443 && protocol === 'https')
    ? ''
    : envPort
  return `${protocol}://${host}${port === 80 || port === 443 ? '' : `:${port}`}/v1${path}`
}

export function restUrl (path: string): string {
  return makeUrl(useTls ? 'https' : 'http', path)
}

export function wsUrl (path: string): string {
  return makeUrl(useTls ? 'wss' : 'ws', path)
}

export async function authUser (name: string, password: string): Promise<string> {
  const resp = await fetch(restUrl('/auth'), {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ name, password }),
  })
  if (!resp.ok) {
    throw new Error(`Authentication failed: ${resp.statusText}`)
  }
  return await resp.text()
}

export async function checkAuth (token: string): Promise<boolean> {
  const resp = await fetch(restUrl('/check_auth'), {
    method: 'GET',
    headers: {
      Authorization: `Bearer ${token}`,
    },
  })

  if (resp.status === 401) {
    return false
  }

  if (!resp.ok) {
    throw new Error(`Authentication check failed: ${resp.statusText}`)
  }

  return true
}

export interface Compteur {
  kwh: number
  cost: number
}

export interface ConsoPeriod {
  start: string
  end: string
  totalKwh: number
  totalCost: number
  compteurs: Record<string, Compteur>
}

export interface ConsoStats {
  dataStart: string
  today: ConsoPeriod
  yesterday?: ConsoPeriod
  thisWeek: ConsoPeriod
  lastWeek?: ConsoPeriod
  thisMonth: ConsoPeriod
  lastMonth?: ConsoPeriod
  thisYear: ConsoPeriod
  lastYear?: ConsoPeriod
}

export async function fetchConsoStats (token: string): Promise<ConsoStats> {
  const resp = await fetch(restUrl('/conso_stats'), {
    method: 'GET',
    headers: {
      Authorization: `Bearer ${token}`,
    },
  })

  if (!resp.ok) {
    throw new Error(`Failed to fetch conso stats: ${resp.statusText}`)
  }

  return await resp.json() as ConsoStats
}
