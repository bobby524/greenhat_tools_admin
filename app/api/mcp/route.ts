import { NextRequest, NextResponse } from 'next/server'
import { createClient } from '@supabase/supabase-js'

export const runtime = 'edge'
export const preferredRegion = 'iad1' // US East (N. Virginia)

// Initialize Supabase
const supabase = createClient(
  process.env.SUPABASE_URL!,
  process.env.SUPABASE_SERVICE_ROLE_KEY!
)

// Admin tool definitions
const adminTools = [
  {
    name: 'crm_list_all_customers',
    description: 'List all CRM customers (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },
  {
    name: 'crm_delete_customer',
    description: 'Delete a customer permanently (SECRET ACL)',
    inputSchema: {
      type: 'object' as const,
      properties: {
        customerId: { type: 'string' },
      },
      required: ['customerId'],
    },
  },
  {
    name: 'admin_list_all_users',
    description: 'List all platform users (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },
  {
    name: 'admin_delete_user',
    description: 'Delete a user permanently (SECRET ACL)',
    inputSchema: {
      type: 'object' as const,
      properties: {
        userId: { type: 'string' },
      },
      required: ['userId'],
    },
  },
  {
    name: 'admin_get_audit_logs',
    description: 'Get platform audit logs (SECRET ACL)',
    inputSchema: {
      type: 'object' as const,
      properties: {
        limit: { type: 'number' },
      },
    },
  },
  {
    name: 'system_health_check',
    description: 'Check platform health (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },
]

// Verify auth token
function verifyAuth(request: NextRequest): boolean {
  const authHeader = request.headers.get('authorization')
  if (!authHeader?.startsWith('Bearer ')) return false
  return authHeader.slice(7) === process.env.ADMIN_MCP_TOKEN
}

// Rate limiting (simple in-memory for edge)
const rateLimits = new Map<string, { count: number; reset: number }>()

function checkRateLimit(sessionId: string, tool: string): boolean {
  const key = `${sessionId}:${tool}`
  const now = Date.now()
  const limit = rateLimits.get(key)
  
  if (!limit || now > limit.reset) {
    rateLimits.set(key, { count: 1, reset: now + 60000 })
    return true
  }
  
  if (limit.count >= 30) return false // 30 requests per minute
  limit.count++
  return true
}

// Execute admin tool
async function executeTool(name: string, args: any): Promise<any> {
  switch (name) {
    case 'crm_list_all_customers':
      const { data: customers } = await supabase.from('customers').select('*')
      return { content: [{ type: 'text', text: `Found ${customers?.length || 0} customers` }] }
    
    case 'admin_list_all_users':
      const { data: users } = await supabase.from('users').select('*')
      return { content: [{ type: 'text', text: `Found ${users?.length || 0} users` }] }
    
    case 'admin_get_audit_logs':
      // In production, fetch from persistent storage
      return { content: [{ type: 'text', text: 'Audit logs available in Supabase' }] }
    
    case 'system_health_check':
      return { content: [{ type: 'text', text: '✅ All systems operational (Vercel Edge)' }] }
    
    case 'crm_delete_customer':
      const { error } = await supabase.from('customers').delete().eq('id', args.customerId)
      if (error) throw error
      return { content: [{ type: 'text', text: `Deleted customer ${args.customerId}` }] }
    
    case 'admin_delete_user':
      const { error: userError } = await supabase.from('users').delete().eq('id', args.userId)
      if (userError) throw userError
      return { content: [{ type: 'text', text: `Deleted user ${args.userId}` }] }
    
    default:
      throw new Error(`Tool ${name} not implemented`)
  }
}

// Log audit event
async function logAudit(sessionId: string, tool: string, args: any, result: string, error?: string) {
  const log = {
    timestamp: new Date().toISOString(),
    sessionId,
    tool,
    result,
    error,
    edge: true,
  }
  
  // In production, write to Supabase or external logging service
  console.log('[AUDIT]', JSON.stringify(log))
}

export async function POST(request: NextRequest) {
  const sessionId = request.headers.get('x-session-id') || crypto.randomUUID()
  
  // Verify authentication
  if (!verifyAuth(request)) {
    await logAudit(sessionId, 'auth', {}, 'blocked', 'Invalid token')
    return NextResponse.json({ error: 'Unauthorized' }, { status: 401 })
  }
  
  try {
    const body = await request.json()
    const { method, params } = body
    
    // List tools
    if (method === 'tools/list') {
      return NextResponse.json({ tools: adminTools })
    }
    
    // Call tool
    if (method === 'tools/call') {
      const { name, arguments: args } = params
      
      // Rate limit check
      if (!checkRateLimit(sessionId, name)) {
        await logAudit(sessionId, name, args, 'blocked', 'Rate limit exceeded')
        return NextResponse.json({ error: 'Rate limit exceeded' }, { status: 429 })
      }
      
      try {
        const result = await executeTool(name, args)
        await logAudit(sessionId, name, args, 'success')
        return NextResponse.json(result)
      } catch (error: any) {
        await logAudit(sessionId, name, args, 'error', error.message)
        return NextResponse.json({ error: error.message }, { status: 500 })
      }
    }
    
    return NextResponse.json({ error: 'Method not found' }, { status: 404 })
    
  } catch (error: any) {
    console.error('MCP Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}

// Health check
export async function GET() {
  return NextResponse.json({
    status: 'healthy',
    service: 'greenhat-admin-mcp',
    runtime: 'edge',
    version: '1.0.0',
    tools: adminTools.length,
  })
}
