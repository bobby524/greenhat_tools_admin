import { NextRequest, NextResponse } from 'next/server'

// Simple API key auth for MCP endpoints
function checkApiKey(request: NextRequest): boolean {
  const authHeader = request.headers.get('authorization')
  if (!authHeader?.startsWith('Bearer ')) return false
  
  const token = authHeader.slice(7)
  return token === process.env.ADMIN_MCP_TOKEN
}

export function middleware(request: NextRequest) {
  // Only protect MCP endpoints
  if (request.nextUrl.pathname.startsWith('/api/mcp')) {
    if (!checkApiKey(request)) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 })
    }
  }
  
  return NextResponse.next()
}

export const config = {
  matcher: ['/api/mcp/:path*']
}
