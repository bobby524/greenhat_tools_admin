import { NextRequest, NextResponse } from 'next/server'
import { createClient } from '@supabase/supabase-js'

export const runtime = 'edge'

// Lazy initialization of Supabase client
function getSupabase() {
  const url = process.env.SUPABASE_URL
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables')
  }
  
  return createClient(url, key)
}

// In-memory session store (edge-compatible)
interface Session {
  id: string
  createdAt: string
  toolCalls: ToolCall[]
  userAgent?: string
  ip?: string
  status: 'active' | 'closed' | 'blocked'
}

interface ToolCall {
  id: string
  tool: string
  timestamp: string
  status: 'success' | 'error' | 'blocked' | 'pending'
  duration?: number
  error?: string
}

// Mock sessions for demo (in production, fetch from Supabase)
const mockSessions: Session[] = [
  {
    id: 'sess-001',
    createdAt: new Date(Date.now() - 3600000).toISOString(),
    status: 'active',
    toolCalls: [
      { id: 'tc-001', tool: 'crm_list_all_customers', timestamp: new Date(Date.now() - 3500000).toISOString(), status: 'success', duration: 150 },
      { id: 'tc-002', tool: 'admin_get_audit_logs', timestamp: new Date(Date.now() - 1800000).toISOString(), status: 'success', duration: 230 },
      { id: 'tc-003', tool: 'system_health_check', timestamp: new Date(Date.now() - 600000).toISOString(), status: 'success', duration: 45 },
    ]
  },
  {
    id: 'sess-002',
    createdAt: new Date(Date.now() - 7200000).toISOString(),
    status: 'closed',
    toolCalls: [
      { id: 'tc-004', tool: 'crm_delete_customer', timestamp: new Date(Date.now() - 7100000).toISOString(), status: 'blocked', error: 'Rate limit exceeded' },
    ]
  },
  {
    id: 'sess-003',
    createdAt: new Date(Date.now() - 1800000).toISOString(),
    status: 'active',
    toolCalls: [
      { id: 'tc-005', tool: 'admin_list_all_users', timestamp: new Date(Date.now() - 1700000).toISOString(), status: 'success', duration: 120 },
      { id: 'tc-006', tool: 'admin_list_all_users', timestamp: new Date(Date.now() - 900000).toISOString(), status: 'success', duration: 115 },
      { id: 'tc-007', tool: 'admin_list_all_users', timestamp: new Date(Date.now() - 300000).toISOString(), status: 'pending' },
    ]
  },
]

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url)
    const status = searchParams.get('status') // 'active', 'closed', 'blocked', or null for all
    const since = searchParams.get('since') // ISO date string
    
    let sessions = [...mockSessions]
    
    // Filter by status
    if (status) {
      sessions = sessions.filter(s => s.status === status)
    }
    
    // Filter by time
    if (since) {
      const sinceDate = new Date(since).getTime()
      sessions = sessions.filter(s => new Date(s.createdAt).getTime() >= sinceDate)
    }
    
    // Calculate stats
    const stats = {
      total: sessions.length,
      active: sessions.filter(s => s.status === 'active').length,
      blocked: sessions.filter(s => s.status === 'blocked').length,
      totalCalls: sessions.reduce((acc, s) => acc + s.toolCalls.length, 0),
      blockedCalls: sessions.reduce((acc, s) => acc + s.toolCalls.filter(tc => tc.status === 'blocked').length, 0),
    }
    
    return NextResponse.json({ 
      sessions,
      stats,
      updatedAt: new Date().toISOString(),
    })
    
  } catch (error: any) {
    console.error('Sessions API Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}