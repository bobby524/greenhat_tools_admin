import { NextRequest, NextResponse } from 'next/server'

export const runtime = 'edge'

interface ToolPermission {
  enabled: boolean
  writeOperation: boolean
  readPrivateData: boolean
  readUntrustedPublicData: boolean
  externalCommunication: boolean
  acl: 'SECRET' | 'PRIVATE' | 'PUBLIC'
  rateLimit?: {
    requestsPerMinute: number
    requestsPerHour: number
  }
}

interface FirewallConfig {
  defaultPolicy: 'allow' | 'deny'
  enableRateLimiting: boolean
  enableDataLeakPrevention: boolean
  enableLethalTrifectaProtection: boolean
  toolPermissions: Record<string, ToolPermission>
  blockedPatterns: string[]
}

// Default configuration
const defaultConfig: FirewallConfig = {
  defaultPolicy: 'deny',
  enableRateLimiting: true,
  enableDataLeakPrevention: true,
  enableLethalTrifectaProtection: true,
  blockedPatterns: [
    'drop table',
    'delete from',
    'rm -rf',
    'exec(',
    'eval(',
    'system(',
    'shell_exec',
    '<script',
  ],
  toolPermissions: {
    // CRM Admin Tools
    crm_list_all_customers: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    crm_delete_customer: {
      enabled: true,
      writeOperation: true,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
    },
    crm_export_customers: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
    },
    
    // Exponential Admin Tools
    admin_list_all_users: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    admin_delete_user: {
      enabled: true,
      writeOperation: true,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
    },
    admin_get_audit_logs: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 30, requestsPerHour: 500 },
    },
    
    // System Admin Tools
    system_export_database: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 2, requestsPerHour: 20 },
    },
    system_health_check: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    
    // Linear/Kanban Board Tools
    boards_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    boards_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    boards_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    boards_archive: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    boards_delete: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
    },
    cards_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 100, requestsPerHour: 2000 },
    },
    cards_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 30, requestsPerHour: 500 },
    },
    cards_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: true,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    cards_move: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 50, requestsPerHour: 800 },
    },
    cards_delete: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    lists_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    lists_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 30, requestsPerHour: 300 },
    },
    lists_delete: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    projects_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    projects_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    projects_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    projects_delete: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
    },
    cycles_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    cycles_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    cycles_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    teams_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    teams_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    workflow_states_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    workflow_states_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    views_list: {
      enabled: true,
      writeOperation: false,
      readPrivateData: false,
      readUntrustedPublicData: true,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
    },
    views_create: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
    views_update: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 20, requestsPerHour: 200 },
    },
    views_delete: {
      enabled: true,
      writeOperation: true,
      readPrivateData: false,
      readUntrustedPublicData: false,
      externalCommunication: false,
      acl: 'SECRET',
      rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
    },
  },
}

// In-memory config store (in production, use Redis or Supabase)
let currentConfig: FirewallConfig = { ...defaultConfig }

export async function GET() {
  return NextResponse.json({
    config: currentConfig,
    updatedAt: new Date().toISOString(),
  })
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json()
    const { toolPermissions, globalSettings } = body
    
    // Update tool permissions
    if (toolPermissions) {
      currentConfig.toolPermissions = {
        ...currentConfig.toolPermissions,
        ...toolPermissions,
      }
    }
    
    // Update global settings
    if (globalSettings) {
      currentConfig = {
        ...currentConfig,
        ...globalSettings,
      }
    }
    
    return NextResponse.json({
      success: true,
      config: currentConfig,
      updatedAt: new Date().toISOString(),
    })
    
  } catch (error: any) {
    console.error('Firewall Config API Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}

export async function PATCH(request: NextRequest) {
  try {
    const body = await request.json()
    const { toolName, updates } = body
    
    if (!toolName || !currentConfig.toolPermissions[toolName]) {
      return NextResponse.json({ error: 'Tool not found' }, { status: 404 })
    }
    
    // Update specific tool
    currentConfig.toolPermissions[toolName] = {
      ...currentConfig.toolPermissions[toolName],
      ...updates,
    }
    
    return NextResponse.json({
      success: true,
      tool: currentConfig.toolPermissions[toolName],
      updatedAt: new Date().toISOString(),
    })
    
  } catch (error: any) {
    console.error('Firewall Config API Error:', error)
    return NextResponse.json({ error: error.message }, { status: 500 })
  }
}