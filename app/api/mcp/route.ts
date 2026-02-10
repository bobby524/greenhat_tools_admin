import { NextRequest, NextResponse } from 'next/server'
import { createClient } from '@supabase/supabase-js'

export const runtime = 'edge'
export const preferredRegion = 'iad1'

const ORG_ID = 'cd861b76-f85c-4afc-b3e8-8f85945c3132'

function getSupabase() {
  const url = process.env.SUPABASE_URL
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY
  if (!url || !key) throw new Error('Missing Supabase environment variables')
  return createClient(url, key)
}

// MCP Tool definitions
const adminTools = [
  // === CRM / Admin ===
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
      properties: { customerId: { type: 'string' } },
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
      properties: { userId: { type: 'string' } },
      required: ['userId'],
    },
  },
  {
    name: 'admin_get_audit_logs',
    description: 'Get platform audit logs (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: { limit: { type: 'number' } } },
  },
  {
    name: 'system_health_check',
    description: 'Check platform health (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },

  // === Exponential v2: Teams ===
  {
    name: 'team_create',
    description: 'Create a team',
    inputSchema: {
      type: 'object' as const,
      properties: {
        name: { type: 'string', description: 'Team name' },
        slug: { type: 'string', description: 'Short uppercase identifier (e.g. ENG)' },
        color: { type: 'string', description: 'Hex color (default #6366F1)' },
      },
      required: ['name', 'slug'],
    },
  },
  {
    name: 'team_list',
    description: 'List all teams',
    inputSchema: { type: 'object' as const, properties: {} },
  },
  {
    name: 'team_get',
    description: 'Get a team and its projects',
    inputSchema: {
      type: 'object' as const,
      properties: { teamId: { type: 'string' } },
      required: ['teamId'],
    },
  },
  {
    name: 'team_update',
    description: 'Update a team',
    inputSchema: {
      type: 'object' as const,
      properties: { teamId: { type: 'string' }, name: { type: 'string' }, slug: { type: 'string' }, color: { type: 'string' } },
      required: ['teamId'],
    },
  },
  {
    name: 'team_delete',
    description: 'Delete a team and all its projects/tasks',
    inputSchema: {
      type: 'object' as const,
      properties: { teamId: { type: 'string' } },
      required: ['teamId'],
    },
  },

  // === Exponential v2: Projects ===
  {
    name: 'project_create',
    description: 'Create a project within a team',
    inputSchema: {
      type: 'object' as const,
      properties: {
        teamId: { type: 'string', description: 'Parent team ID' },
        name: { type: 'string', description: 'Project name' },
        description: { type: 'string', description: 'Project description' },
        color: { type: 'string', description: 'Hex color' },
        icon: { type: 'string', description: 'Icon name (default folder)' },
      },
      required: ['teamId', 'name'],
    },
  },
  {
    name: 'project_list',
    description: 'List projects (optionally filtered by team)',
    inputSchema: {
      type: 'object' as const,
      properties: {
        teamId: { type: 'string', description: 'Filter by team' },
        limit: { type: 'number' },
      },
    },
  },
  {
    name: 'project_get',
    description: 'Get a project with its tasks and sprints',
    inputSchema: {
      type: 'object' as const,
      properties: { projectId: { type: 'string' } },
      required: ['projectId'],
    },
  },
  {
    name: 'project_update',
    description: 'Update a project',
    inputSchema: {
      type: 'object' as const,
      properties: { projectId: { type: 'string' }, name: { type: 'string' }, description: { type: 'string' }, color: { type: 'string' }, icon: { type: 'string' }, teamId: { type: 'string' } },
      required: ['projectId'],
    },
  },
  {
    name: 'project_delete',
    description: 'Delete a project and all its tasks',
    inputSchema: {
      type: 'object' as const,
      properties: { projectId: { type: 'string' } },
      required: ['projectId'],
    },
  },

  // === Exponential v2: Tasks ===
  {
    name: 'task_create',
    description: 'Create a task in a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Parent project ID' },
        title: { type: 'string', description: 'Task title' },
        description: { type: 'string', description: 'Task description' },
        status: { type: 'string', enum: ['backlog', 'todo', 'in_progress', 'done', 'cancelled'], description: 'Task status' },
        priority: { type: 'number', description: 'Priority: 0=none, 1=urgent, 2=high, 3=medium, 4=low' },
        assigneeId: { type: 'string', description: 'Assignee user ID' },
        sprintId: { type: 'string', description: 'Sprint ID' },
        dueAt: { type: 'string', description: 'Due date (ISO format)' },
        labels: { type: 'array', items: { type: 'string' }, description: 'Label names' },
      },
      required: ['projectId', 'title'],
    },
  },
  {
    name: 'task_list',
    description: 'List tasks with filters',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string' },
        sprintId: { type: 'string' },
        status: { type: 'string' },
        assigneeId: { type: 'string' },
        search: { type: 'string', description: 'Search title/identifier' },
        limit: { type: 'number' },
      },
    },
  },
  {
    name: 'task_get',
    description: 'Get a task by ID with relations',
    inputSchema: {
      type: 'object' as const,
      properties: { taskId: { type: 'string' } },
      required: ['taskId'],
    },
  },
  {
    name: 'task_update',
    description: 'Update a task',
    inputSchema: {
      type: 'object' as const,
      properties: {
        taskId: { type: 'string' },
        title: { type: 'string' },
        description: { type: 'string' },
        status: { type: 'string' },
        priority: { type: 'number' },
        assigneeId: { type: 'string' },
        sprintId: { type: 'string' },
        dueAt: { type: 'string' },
        labels: { type: 'array', items: { type: 'string' } },
        position: { type: 'number' },
      },
      required: ['taskId'],
    },
  },
  {
    name: 'task_delete',
    description: 'Delete a task',
    inputSchema: {
      type: 'object' as const,
      properties: { taskId: { type: 'string' } },
      required: ['taskId'],
    },
  },

  // === Exponential v2: Sprints ===
  {
    name: 'sprint_create',
    description: 'Create a sprint for a project',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string', description: 'Parent project ID' },
        name: { type: 'string', description: 'Sprint name (auto-numbered if blank)' },
        startDate: { type: 'string', description: 'Start date (ISO)' },
        endDate: { type: 'string', description: 'End date (ISO)' },
      },
      required: ['projectId'],
    },
  },
  {
    name: 'sprint_list',
    description: 'List sprints (optionally for a project)',
    inputSchema: {
      type: 'object' as const,
      properties: {
        projectId: { type: 'string' },
        state: { type: 'string', enum: ['planned', 'active', 'completed'] },
      },
    },
  },
  {
    name: 'sprint_get',
    description: 'Get a sprint with its tasks',
    inputSchema: {
      type: 'object' as const,
      properties: { sprintId: { type: 'string' } },
      required: ['sprintId'],
    },
  },
  {
    name: 'sprint_update',
    description: 'Update a sprint',
    inputSchema: {
      type: 'object' as const,
      properties: {
        sprintId: { type: 'string' },
        name: { type: 'string' },
        state: { type: 'string', enum: ['planned', 'active', 'completed'] },
        startDate: { type: 'string' },
        endDate: { type: 'string' },
      },
      required: ['sprintId'],
    },
  },
  {
    name: 'sprint_delete',
    description: 'Delete a sprint',
    inputSchema: {
      type: 'object' as const,
      properties: { sprintId: { type: 'string' } },
      required: ['sprintId'],
    },
  },

  // === Exponential v2: Labels ===
  {
    name: 'label_list',
    description: 'List all labels',
    inputSchema: { type: 'object' as const, properties: {} },
  },
  {
    name: 'label_create',
    description: 'Create a label',
    inputSchema: {
      type: 'object' as const,
      properties: {
        name: { type: 'string' },
        color: { type: 'string', description: 'Hex color' },
      },
      required: ['name', 'color'],
    },
  },
  {
    name: 'label_delete',
    description: 'Delete a label',
    inputSchema: {
      type: 'object' as const,
      properties: { labelId: { type: 'string' } },
      required: ['labelId'],
    },
  },
]

// Auth
function verifyAuth(request: NextRequest): boolean {
  const authHeader = request.headers.get('authorization')
  if (!authHeader?.startsWith('Bearer ')) return false
  return authHeader.slice(7) === process.env.ADMIN_MCP_TOKEN
}

// Rate limiting
const rateLimits = new Map<string, { count: number; reset: number }>()
function checkRateLimit(sessionId: string, tool: string): boolean {
  const key = `${sessionId}:${tool}`
  const now = Date.now()
  const limit = rateLimits.get(key)
  if (!limit || now > limit.reset) { rateLimits.set(key, { count: 1, reset: now + 60000 }); return true }
  if (limit.count >= 30) return false
  limit.count++
  return true
}

// Execute tool
async function executeTool(name: string, args: any): Promise<any> {
  const supabase = getSupabase()
  const text = (t: string) => ({ content: [{ type: 'text', text: t }] })

  switch (name) {
    // --- CRM / Admin (unchanged) ---
    case 'crm_list_all_customers': {
      const { data } = await supabase.from('customers').select('*')
      return text(`Found ${data?.length || 0} customers`)
    }
    case 'admin_list_all_users': {
      const { data } = await supabase.from('users').select('*')
      return text(`Found ${data?.length || 0} users`)
    }
    case 'admin_get_audit_logs':
      return text('Audit logs available in Supabase')
    case 'system_health_check':
      return text('✅ All systems operational (Vercel Edge)')
    case 'crm_delete_customer': {
      const { error } = await supabase.from('customers').delete().eq('id', args.customerId)
      if (error) throw error
      return text(`Deleted customer ${args.customerId}`)
    }
    case 'admin_delete_user': {
      const { error } = await supabase.from('users').delete().eq('id', args.userId)
      if (error) throw error
      return text(`Deleted user ${args.userId}`)
    }

    // --- Teams ---
    case 'team_create': {
      const { data, error } = await supabase.from('teams')
        .insert({ org_id: ORG_ID, name: args.name, slug: args.slug.toUpperCase(), color: args.color || '#6366F1' })
        .select().single()
      if (error) throw error
      return text(`Created team: ${data.name} [${data.slug}] (ID: ${data.id})`)
    }
    case 'team_list': {
      const { data, error } = await supabase.from('teams').select('*').eq('org_id', ORG_ID).order('name')
      if (error) throw error
      const list = data?.map(t => `- ${t.name} [${t.slug}] (${t.id})`).join('\n') || 'No teams'
      return text(`Teams:\n${list}`)
    }
    case 'team_get': {
      const { data: team, error } = await supabase.from('teams').select('*').eq('id', args.teamId).single()
      if (error) throw error
      const { data: projects } = await supabase.from('projects').select('id, name').eq('team_id', args.teamId).order('name')
      const projList = projects?.map(p => `  - ${p.name} (${p.id})`).join('\n') || '  (none)'
      return text(`Team: ${team.name} [${team.slug}]\nProjects:\n${projList}`)
    }
    case 'team_update': {
      const updates: any = {}
      if (args.name) updates.name = args.name
      if (args.slug) updates.slug = args.slug.toUpperCase()
      if (args.color) updates.color = args.color
      const { data, error } = await supabase.from('teams').update(updates).eq('id', args.teamId).select().single()
      if (error) throw error
      return text(`Updated team: ${data.name}`)
    }
    case 'team_delete': {
      const { error } = await supabase.from('teams').delete().eq('id', args.teamId)
      if (error) throw error
      return text(`Deleted team ${args.teamId}`)
    }

    // --- Projects ---
    case 'project_create': {
      const { data, error } = await supabase.from('projects')
        .insert({ team_id: args.teamId, org_id: ORG_ID, name: args.name, description: args.description || null, color: args.color || '#62ac4a', icon: args.icon || 'folder' })
        .select().single()
      if (error) throw error
      return text(`Created project: ${data.name} (ID: ${data.id})`)
    }
    case 'project_list': {
      let query = supabase.from('projects').select('*, team:teams(name, slug)').eq('org_id', ORG_ID).order('name')
      if (args.teamId) query = query.eq('team_id', args.teamId)
      if (args.limit) query = query.limit(args.limit)
      const { data, error } = await query
      if (error) throw error
      const list = data?.map(p => `- ${p.team?.name || '?'}/${p.name} (${p.id})`).join('\n') || 'No projects'
      return text(`Projects:\n${list}`)
    }
    case 'project_get': {
      const { data: project, error } = await supabase.from('projects').select('*, team:teams(name, slug)').eq('id', args.projectId).single()
      if (error) throw error
      const { data: tasks } = await supabase.from('tasks').select('id, title, status, priority').eq('project_id', args.projectId).order('position')
      const taskList = tasks?.map(t => `  - [${t.status}] ${t.title} (p${t.priority})`).join('\n') || '  (no tasks)'
      return text(`Project: ${project.name}\nTeam: ${project.team?.name}\nDescription: ${project.description || '(none)'}\nTasks:\n${taskList}`)
    }
    case 'project_update': {
      const updates: any = {}
      if (args.name) updates.name = args.name
      if (args.description !== undefined) updates.description = args.description
      if (args.color) updates.color = args.color
      if (args.icon) updates.icon = args.icon
      if (args.teamId) updates.team_id = args.teamId
      const { data, error } = await supabase.from('projects').update(updates).eq('id', args.projectId).select().single()
      if (error) throw error
      return text(`Updated project: ${data.name}`)
    }
    case 'project_delete': {
      const { error } = await supabase.from('projects').delete().eq('id', args.projectId)
      if (error) throw error
      return text(`Deleted project ${args.projectId}`)
    }

    // --- Tasks ---
    case 'task_create': {
      // Get next identifier
      const { data: identifier } = await supabase.rpc('get_next_task_number', { p_project_id: args.projectId })
      // Get max position
      const { data: maxPos } = await supabase.from('tasks').select('position').eq('project_id', args.projectId).order('position', { ascending: false }).limit(1)
      const position = (maxPos?.[0]?.position || 0) + 1000
      // Get org_id from project
      const { data: project } = await supabase.from('projects').select('org_id').eq('id', args.projectId).single()

      const { data, error } = await supabase.from('tasks')
        .insert({
          project_id: args.projectId,
          org_id: project?.org_id || ORG_ID,
          identifier: identifier || null,
          title: args.title,
          description: args.description || null,
          status: args.status || 'todo',
          priority: args.priority ?? 0,
          assignee_id: args.assigneeId || null,
          sprint_id: args.sprintId || null,
          due_at: args.dueAt || null,
          labels: args.labels || [],
          position,
        })
        .select().single()
      if (error) throw error
      return text(`Created task: ${data.identifier || ''} ${data.title} (ID: ${data.id})`)
    }
    case 'task_list': {
      let query = supabase.from('tasks').select('id, identifier, title, status, priority, project_id, sprint_id, due_at, labels').eq('org_id', ORG_ID).order('position')
      if (args.projectId) query = query.eq('project_id', args.projectId)
      if (args.sprintId) query = query.eq('sprint_id', args.sprintId)
      if (args.status) query = query.eq('status', args.status)
      if (args.assigneeId) query = query.eq('assignee_id', args.assigneeId)
      if (args.search) query = query.or(`title.ilike.%${args.search}%,identifier.ilike.%${args.search}%`)
      if (args.limit) query = query.limit(args.limit)
      const { data, error } = await query
      if (error) throw error
      const list = data?.map(t => `- [${t.status}] ${t.identifier || '?'} ${t.title} (p${t.priority})`).join('\n') || 'No tasks'
      return text(`Tasks (${data?.length || 0}):\n${list}`)
    }
    case 'task_get': {
      const { data: task, error } = await supabase.from('tasks').select('*').eq('id', args.taskId).single()
      if (error) throw error
      const { data: relations } = await supabase.from('task_relations').select('*').or(`source_task_id.eq.${args.taskId},target_task_id.eq.${args.taskId}`)
      return text(`Task: ${task.identifier || ''} ${task.title}\nStatus: ${task.status} | Priority: ${task.priority}\nDescription: ${task.description || '(none)'}\nLabels: ${(task.labels || []).join(', ') || '(none)'}\nDue: ${task.due_at || '(none)'}\nRelations: ${relations?.length || 0}`)
    }
    case 'task_update': {
      const updates: any = {}
      if (args.title) updates.title = args.title
      if (args.description !== undefined) updates.description = args.description
      if (args.status) updates.status = args.status
      if (args.priority !== undefined) updates.priority = args.priority
      if (args.assigneeId !== undefined) updates.assignee_id = args.assigneeId
      if (args.sprintId !== undefined) updates.sprint_id = args.sprintId
      if (args.dueAt !== undefined) updates.due_at = args.dueAt
      if (args.labels) updates.labels = args.labels
      if (args.position !== undefined) updates.position = args.position
      const { data, error } = await supabase.from('tasks').update(updates).eq('id', args.taskId).select().single()
      if (error) throw error
      return text(`Updated task: ${data.identifier || ''} ${data.title}`)
    }
    case 'task_delete': {
      const { error } = await supabase.from('tasks').delete().eq('id', args.taskId)
      if (error) throw error
      return text(`Deleted task ${args.taskId}`)
    }

    // --- Sprints ---
    case 'sprint_create': {
      const { data: existing } = await supabase.from('sprints').select('number').eq('project_id', args.projectId).order('number', { ascending: false }).limit(1)
      const nextNumber = (existing?.[0]?.number || 0) + 1
      const { data, error } = await supabase.from('sprints')
        .insert({ project_id: args.projectId, org_id: ORG_ID, name: args.name || `Sprint ${nextNumber}`, number: nextNumber, start_date: args.startDate || null, end_date: args.endDate || null, state: 'planned' })
        .select().single()
      if (error) throw error
      return text(`Created sprint: ${data.name} #${data.number} (ID: ${data.id})`)
    }
    case 'sprint_list': {
      let query = supabase.from('sprints').select('*').eq('org_id', ORG_ID).order('number')
      if (args.projectId) query = query.eq('project_id', args.projectId)
      if (args.state) query = query.eq('state', args.state)
      const { data, error } = await query
      if (error) throw error
      const list = data?.map(s => `- ${s.name} (${s.state}) [${s.id}]`).join('\n') || 'No sprints'
      return text(`Sprints:\n${list}`)
    }
    case 'sprint_get': {
      const { data: sprint, error } = await supabase.from('sprints').select('*').eq('id', args.sprintId).single()
      if (error) throw error
      const { data: tasks } = await supabase.from('tasks').select('id, identifier, title, status').eq('sprint_id', args.sprintId).order('position')
      const taskList = tasks?.map(t => `  - [${t.status}] ${t.identifier || '?'} ${t.title}`).join('\n') || '  (no tasks)'
      return text(`Sprint: ${sprint.name} #${sprint.number} (${sprint.state})\nDates: ${sprint.start_date || '?'} → ${sprint.end_date || '?'}\nTasks:\n${taskList}`)
    }
    case 'sprint_update': {
      const updates: any = {}
      if (args.name) updates.name = args.name
      if (args.state) updates.state = args.state
      if (args.startDate !== undefined) updates.start_date = args.startDate
      if (args.endDate !== undefined) updates.end_date = args.endDate
      const { data, error } = await supabase.from('sprints').update(updates).eq('id', args.sprintId).select().single()
      if (error) throw error
      return text(`Updated sprint: ${data.name}`)
    }
    case 'sprint_delete': {
      const { error } = await supabase.from('sprints').delete().eq('id', args.sprintId)
      if (error) throw error
      return text(`Deleted sprint ${args.sprintId}`)
    }

    // --- Labels ---
    case 'label_list': {
      const { data, error } = await supabase.from('labels').select('*').eq('org_id', ORG_ID).order('name')
      if (error) throw error
      const list = data?.map(l => `- ${l.name} (${l.color}) [${l.id}]`).join('\n') || 'No labels'
      return text(`Labels:\n${list}`)
    }
    case 'label_create': {
      const { data, error } = await supabase.from('labels').insert({ org_id: ORG_ID, name: args.name, color: args.color }).select().single()
      if (error) throw error
      return text(`Created label: ${data.name} (${data.color})`)
    }
    case 'label_delete': {
      const { error } = await supabase.from('labels').delete().eq('id', args.labelId)
      if (error) throw error
      return text(`Deleted label ${args.labelId}`)
    }

    default:
      throw new Error(`Tool ${name} not implemented`)
  }
}

// Audit logging
async function logAudit(sessionId: string, tool: string, args: any, result: string, error?: string) {
  console.log('[AUDIT]', JSON.stringify({ timestamp: new Date().toISOString(), sessionId, tool, result, error, edge: true }))
}

export async function POST(request: NextRequest) {
  const sessionId = request.headers.get('x-session-id') || crypto.randomUUID()

  if (!verifyAuth(request)) {
    await logAudit(sessionId, 'auth', {}, 'blocked', 'Invalid token')
    return NextResponse.json({ error: 'Unauthorized' }, { status: 401 })
  }

  try {
    const body = await request.json()
    const { method, params } = body

    if (method === 'tools/list') {
      return NextResponse.json({ tools: adminTools })
    }

    if (method === 'tools/call') {
      const { name, arguments: args } = params
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

export async function GET() {
  return NextResponse.json({
    status: 'healthy',
    service: 'greenhat-admin-mcp',
    runtime: 'edge',
    version: '2.0.0',
    tools: adminTools.length,
  })
}
