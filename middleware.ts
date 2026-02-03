import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'

// IP whitelist (optional)
const ALLOWED_IPS = process.env.ALLOWED_IPS?.split(',').filter(Boolean) || []

// Simple password check
function checkPassword(request: NextRequest): boolean {
  const authHeader = request.headers.get('authorization')
  if (!authHeader?.startsWith('Basic ')) return false
  
  const credentials = atob(authHeader.slice(6))
  const [username, password] = credentials.split(':')
  
  const expectedUsername = process.env.ADMIN_USERNAME || 'admin'
  // Support both ADMIN_PASSWORD and ADMIN_PASSWORD_HASH for backwards compatibility
  const expectedPassword = process.env.ADMIN_PASSWORD || process.env.ADMIN_PASSWORD_HASH
  
  return username === expectedUsername && password === expectedPassword
}

export function middleware(request: NextRequest) {
  // Check IP whitelist if configured
  if (ALLOWED_IPS.length > 0) {
    const ip = request.ip || request.headers.get('x-forwarded-for')?.split(',')[0]
    if (ip && !ALLOWED_IPS.includes(ip)) {
      return new NextResponse('Access denied: IP not allowed', { status: 403 })
    }
  }
  
  // Check password for admin routes
  if (request.nextUrl.pathname.startsWith('/admin')) {
    if (!checkPassword(request)) {
      return new NextResponse('Authentication required', { 
        status: 401,
        headers: {
          'WWW-Authenticate': 'Basic realm="Admin"'
        }
      })
    }
  }
  
  // Add security headers
  const response = NextResponse.next()
  response.headers.set('X-Frame-Options', 'DENY')
  response.headers.set('X-Content-Type-Options', 'nosniff')
  response.headers.set('Referrer-Policy', 'strict-origin-when-cross-origin')
  
  return response
}

export const config = {
  matcher: ['/admin/:path*', '/api/mcp/:path*']
}
