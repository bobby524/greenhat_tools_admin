import { NextRequest, NextResponse } from 'next/server'

// Simple test endpoint first
export async function GET(request: NextRequest) {
  console.log('Audit API called at:', new Date().toISOString())
  
  try {
    return NextResponse.json({ 
      message: 'Audit API is working',
      timestamp: new Date().toISOString(),
      env: {
        hasUrl: !!process.env.NEXT_PUBLIC_SUPABASE_URL,
        hasKey: !!process.env.SUPABASE_SERVICE_ROLE_KEY
      }
    })
  } catch (error: any) {
    console.error('Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}

// Initialize Supabase client
const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL!
const supabaseServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY!
const supabase = createClient(supabaseUrl, supabaseServiceKey)

export async function GET(request: NextRequest) {
  console.log('Audit API called')
  try {
    const { searchParams } = new URL(request.url)
    const since = searchParams.get('since')
    const action = searchParams.get('action')
    const status = searchParams.get('status')
    const tool = searchParams.get('tool')
    const limit = parseInt(searchParams.get('limit') || '50')
    
    // Build the query
    let query = supabase
      .from('mcp_audit_logs')
      .select('*')
      .order('created_at', { ascending: false })
      .limit(limit)
    
    // Apply filters
    if (since) {
      query = query.gte('created_at', since)
    }
    
    if (action) {
      query = query.eq('action', action)
    }
    
    if (status) {
      query = query.eq('status', status)
    }
    
    if (tool) {
      query = query.eq('tool_name', tool)
    }
    
    const { data: logs, error } = await query
    
    if (error) {
      console.error('Supabase error:', error)
      return NextResponse.json({ error: error.message }, { status: 500 })
    }
    
    // Get stats
    const { data: statsData, error: statsError } = await supabase
      .from('mcp_audit_logs')
      .select('status', { count: 'exact' })
    
    if (statsError) {
      console.error('Stats error:', statsError)
    }
    
    // Calculate stats
    const total = logs?.length || 0
    const blocked = logs?.filter((l: any) => l.status === 'blocked').length || 0
    const errors = logs?.filter((l: any) => l.status === 'error').length || 0
    const allowed = logs?.filter((l: any) => l.status === 'allowed').length || 0
    
    // Get last 24h count
    const last24h = logs?.filter((l: any) => {
      const logTime = new Date(l.created_at).getTime()
      return logTime > Date.now() - 86400000
    }).length || 0
    
    const stats = {
      total,
      blocked,
      errors,
      allowed,
      last24h,
    }
    
    // Transform logs to match the expected format
    const transformedLogs = logs?.map((log: any) => ({
      id: log.id,
      timestamp: log.created_at,
      sessionId: log.session_id,
      tool: log.tool_name,
      action: log.action,
      status: log.status === 'allowed' ? 'success' : log.status,
      details: log.error_message || `${log.action} on ${log.path}`,
      metadata: {
        duration_ms: log.duration_ms,
        is_write: log.is_write,
        tool_category: log.tool_category,
        path: log.path,
      },
    })) || []
    
    return NextResponse.json({
      logs: transformedLogs,
      stats,
      updatedAt: new Date().toISOString(),
    })
    
  } catch (error: any) {
    console.error('Audit Logs API Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}
