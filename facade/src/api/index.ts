const serverUrl = import.meta.env.VITE_API_SERVER_URL

export async function authUser (name: string, password: string): Promise<string> {
  const resp = await fetch(`${serverUrl}/v1/auth`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ name, password }),
  })
  console.log(resp)
  return await resp.text()
}
