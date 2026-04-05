const serverHost = import.meta.env.VITE_API_SERVER_HOST || 'localhost:3030'

export function makeUrl(protocol: string, path: string): string {
  return `${protocol}://${serverHost}/v1${path}`
}

export function restUrl(path: string): string {
  return makeUrl('http', path)
}

export function wsUrl(path: string): string {
  return makeUrl('ws', path)
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
