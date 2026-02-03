import { NextRequest, NextResponse } from 'next/server'

export const runtime = 'edge'

interface AuditLog {
  id: string
  timestamp: string
  sessionId: string
  tool: string
  action: 'call' | 'block' | 'error' | 'auth' | 'config_change'
  status: 'success' | 'blocked' | 'error' | 'warning'
  details?: string
  ip?: string
  userAgent?: string
  metadata?: Record<string, any>
}

// Mock audit logs (in production, fetch from Supabase or logging service)
const mockAuditLogs: AuditLog[] = [
  {
    id: 'log-001',
    timestamp: new Date(Date.now() - 3500000).toISOString(),
    sessionId: 'sess-001',
    tool: 'crm_list_all_customers',
    action: 'call',
    status: 'success',
    details: 'Retrieved 47 customers',
    ip: '10.0.0.1',
  },
  {
    id: 'log-002',
    timestamp: new Date(Date.now() - 3200000).toISOString(),
    sessionId: 'sess-001',
    tool: 'crm_delete_customer',
    action: 'block',
    status: 'blocked',
    details: 'Rate limit exceeded: 15 requests/minute',
    ip: '10.0.0.1',
    metadata: { rateLimit: 10, window: '1m' }
  },
  {
    id: 'log-003',
    timestamp: new Date(Date.now() - 1800000).toISOString(),
    sessionId: 'sess-001',
    tool: 'admin_get_audit_logs',
    action: 'call',
    status: 'success',
    details: 'Retrieved 150 logs',
    ip: '10.0.0.1',
  },
  {
    id: 'log-004',
    timestamp: new Date(Date.now() - 7100000).toISOString(),
    sessionId: 'sess-002',
    tool: 'crm_delete_customer',
    action: 'block',
    status: 'blocked',
    details: 'Lethal trifecta detected: write + private data + destructive operation',
    ip: '10.0.0.2',
    metadata: { flags: ['writeOperation', 'readPrivateData', 'destructive'] }
  },
  {
    id: 'log-005',
    timestamp: new Date(Date.now() - 600000).toISOString(),
    sessionId: 'sess-001',
    tool: 'system_health_check',
    action: 'call',
    status: 'success',
    details: 'All systems operational',
    ip: '10.0.0.1',
  },
  {
    id: 'log-006',
    timestamp: new Date(Date.now() - 300000).toISOString(),
    sessionId: 'sess-003',
    tool: 'admin_list_all_users',
    action: 'call',
    status: 'warning',
    details: 'High frequency detected: 3 calls in 5 minutes',
    ip: '10.0.0.3',
    metadata: { frequency: 'high', callsInWindow: 3 }
  },
  {
    id: 'log-007',
    timestamp: new Date(Date.now() - 120000).toISOString(),
    sessionId: 'system',
    tool: 'firewall_config',
    action: 'config_change',
    status: 'success',
    details: 'Updated tool permissions for crm_delete_customer',
    metadata: { changedBy: 'admin', changes: ['rateLimit'] }
  },
]

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url)
    const since = searchParams.get('since')
    const action = searchParams.get('action')
    const status = searchParams.get('status')
    const tool = searchParams.get('tool')
    const limit = parseInt(searchParams.get('limit') || '50')
    
    let logs = [...mockAuditLogs]
    
    // Apply filters
    if (since) {
      const sinceDate = new Date(since).getTime()
      logs = logs.filter(l => new Date(l.timestamp).getTime() >= sinceDate)
    }
    
    if (action) {
      logs = logs.filter(l => l.action === action)
    }
    
    if (status) {
      logs = logs.filter(l => l.status === status)
    }
    
    if (tool) {
      logs = logs.filter(l => l.tool === tool)
    }
    
    // Sort by timestamp descending
    logs.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
    
    // Apply limit
    logs = logs.slice(0, limit)
    
    // Calculate stats
    const stats = {
      total: mockAuditLogs.length,
      blocked: mockAuditLogs.filter(l => l.status === 'blocked').length,
      errors: mockAuditLogs.filter(l => l.status === 'error').length,
      warnings: mockAuditLogs.filter(l => l.status === 'warning').length,
      last24h: mockAuditLogs.filter(l => new Date(l.timestamp).getTime() > Date.now() - 86400000).length,
    }
    
    return NextResponse.json({
      logs,
      stats,
      updatedAt: new Date().toISOString(),
    })
    
  } catch (error: any) {
    console.error('Audit Logs API Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}