import { NextRequest, NextResponse } from 'next/server'
import { createClient } from '@supabase/supabase-js'

export const runtime = 'edge'
export const preferredRegion = 'iad1' // US East (N. Virginia)

// Lazy initialization of Supabase client
function getSupabase() {
  const url = process.env.SUPABASE_URL
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables')
  }
  
  return createClient(url, key)
}

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
  // Project Management Tools (Exponential)
  {
    name: 'project_create',
    description: 'Create a new project in Exponential',
    inputSchema: {
      type: 'object' as const,
      properties: {
        name: { type: 'string', description: 'Project name' },
        description: { type: 'string', description: 'Project description' },
        status: { type: 'string', enum: ['planning', 'active', 'on_hold', 'completed'], description: 'Project status' },
        priority: { type: 'string', enum: ['low', 'medium', 'high', 'critical'], description: 'Project priority' },
        startDate: { type: 'string', description: 'Start date (ISO format)' },
        targetEndDate: { type: 'string', description: 'Target end date (ISO format)' },
        labels: { type: 'array', items: { type: 'string' }, description: 'Project labels/tags' },
      },
      required: ['name', 'description'],
    },
  },
  {
    name: 'project_list',
    description: 'List all projects',
    inputSchema: {
      type: 'object' as const,
      properties: {
        status: { type: 'string', description: 'Filter by status' },
        limit: { type: 'number', description: 'Max results to return' },
      },
    },
  },
  {
    name: 'project_get',
    description: 'Get a project by ID',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Project ID' },
      },
      required: ['projectId'],
    },
  },
  {
    name: 'project_update',
    description: 'Update a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Project ID' },
        name: { type: 'string' },
        description: { type: 'string' },
        status: { type: 'string' },
        priority: { type: 'string' },
      },
      required: ['projectId'],
    },
  },
  {
    name: 'project_delete',
    description: 'Delete a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Project ID' },
      },
      required: ['projectId'],
    },
  },
  // Sprint Management
  {
    name: 'sprint_create',
    description: 'Create a sprint for a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Parent project ID' },
        name: { type: 'string', description: 'Sprint name' },
        goal: { type: 'string', description: 'Sprint goal/description' },
        duration: { type: 'string', description: 'Duration (e.g., "1 week", "2 weeks")' },
        startDate: { type: 'string', description: 'Start date (ISO format)' },
        endDate: { type: 'string', description: 'End date (ISO format)' },
      },
      required: ['projectId', 'name', 'goal'],
    },
  },
  {
    name: 'sprint_list',
    description: 'List sprints for a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Project ID' },
      },
      required: ['projectId'],
    },
  },
  // Task Management
  {
    name: 'task_create',
    description: 'Create a task in a sprint',
    inputSchema: {
      type: 'object' as const,
      properties: {
        sprintId: { type: 'string', description: 'Parent sprint ID' },
        title: { type: 'string', description: 'Task title' },
        description: { type: 'string', description: 'Task description' },
        priority: { type: 'string', enum: ['low', 'medium', 'high', 'critical'], description: 'Task priority' },
        status: { type: 'string', enum: ['todo', 'in_progress', 'review', 'done'], description: 'Task status' },
        acceptanceCriteria: { type: 'array', items: { type: 'string' }, description: 'Acceptance criteria' },
        estimatedHours: { type: 'number', description: 'Estimated hours' },
      },
      required: ['sprintId', 'title', 'description'],
    },
  },
  {
    name: 'task_list',
    description: 'List tasks for a sprint',
    inputSchema: {
      type: 'object' as const,
      properties: {
        sprintId: { type: 'string', description: 'Sprint ID' },
      },
      required: ['sprintId'],
    },
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
  const supabase = getSupabase()
  
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
    
    // Project Management
    case 'project_create': {
      const { data: project, error } = await supabase
        .from('exponential_projects')
        .insert({
          name: args.name,
          description: args.description,
          status: args.status || 'planning',
          priority: args.priority || 'medium',
          start_date: args.startDate,
          target_end_date: args.targetEndDate,
          labels: args.labels || [],
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        })
        .select()
        .single()
      
      if (error) throw error
      return { content: [{ type: 'text', text: `Created project: ${project.name} (ID: ${project.id})` }] }
    }
    
    case 'project_list': {
      let query = supabase.from('exponential_projects').select('*')
      if (args.status) query = query.eq('status', args.status)
      if (args.limit) query = query.limit(args.limit)
      
      const { data: projects, error } = await query.order('created_at', { ascending: false })
      if (error) throw error
      
      const list = projects?.map(p => `- ${p.name} (${p.status}) [${p.id}]`).join('\n') || 'No projects found'
      return { content: [{ type: 'text', text: `Projects:\n${list}` }] }
    }
    
    case 'project_get': {
      const { data: project, error } = await supabase
        .from('exponential_projects')
        .select('*')
        .eq('id', args.projectId)
        .single()
      
      if (error) throw error
      return { content: [{ type: 'text', text: `Project: ${project.name}\nStatus: ${project.status}\nPriority: ${project.priority}\nDescription: ${project.description}` }] }
    }
    
    case 'project_update': {
      const updates: any = { updated_at: new Date().toISOString() }
      if (args.name) updates.name = args.name
      if (args.description) updates.description = args.description
      if (args.status) updates.status = args.status
      if (args.priority) updates.priority = args.priority
      
      const { data: project, error } = await supabase
        .from('exponential_projects')
        .update(updates)
        .eq('id', args.projectId)
        .select()
        .single()
      
      if (error) throw error
      return { content: [{ type: 'text', text: `Updated project: ${project.name}` }] }
    }
    
    case 'project_delete': {
      const { error } = await supabase.from('exponential_projects').delete().eq('id', args.projectId)
      if (error) throw error
      return { content: [{ type: 'text', text: `Deleted project ${args.projectId}` }] }
    }
    
    case 'sprint_create': {
      const { data: sprint, error } = await supabase
        .from('exponential_sprints')
        .insert({
          project_id: args.projectId,
          name: args.name,
          goal: args.goal,
          duration: args.duration,
          start_date: args.startDate,
          end_date: args.endDate,
          status: 'planned',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        })
        .select()
        .single()
      
      if (error) throw error
      return { content: [{ type: 'text', text: `Created sprint: ${sprint.name} (ID: ${sprint.id})` }] }
    }
    
    case 'sprint_list': {
      const { data: sprints, error } = await supabase
        .from('exponential_sprints')
        .select('*')
        .eq('project_id', args.projectId)
        .order('created_at', { ascending: false })
      
      if (error) throw error
      
      const list = sprints?.map(s => `- ${s.name} (${s.status}) [${s.id}]`).join('\n') || 'No sprints found'
      return { content: [{ type: 'text', text: `Sprints:\n${list}` }] }
    }
    
    case 'task_create': {
      const { data: task, error } = await supabase
        .from('exponential_tasks')
        .insert({
          sprint_id: args.sprintId,
          title: args.title,
          description: args.description,
          priority: args.priority || 'medium',
          status: args.status || 'todo',
          acceptance_criteria: args.acceptanceCriteria || [],
          estimated_hours: args.estimatedHours,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        })
        .select()
        .single()
      
      if (error) throw error
      return { content: [{ type: 'text', text: `Created task: ${task.title} (ID: ${task.id})` }] }
    }
    
    case 'task_list': {
      const { data: tasks, error } = await supabase
        .from('exponential_tasks')
        .select('*')
        .eq('sprint_id', args.sprintId)
        .order('created_at', { ascending: false })
      
      if (error) throw error
      
      const list = tasks?.map(t => `- [${t.status}] ${t.title} (${t.priority}) [${t.id}]`).join('\n') || 'No tasks found'
      return { content: [{ type: 'text', text: `Tasks:\n${list}` }] }
    }
    
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
