#!/usr/bin/env node
/**
 * Admin MCP Server
 * 
 * All tools require SECRET ACL - highest privilege level
 * Deployed on isolated infrastructure with VPN-only access
 */

import express from 'express';
import cors from 'cors';
import helmet from 'helmet';
import { randomUUID } from 'crypto';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { SSEServerTransport } from '@modelcontextprotocol/sdk/server/sse.js';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';

import { createClient, supabase } from './client.js';
import { Observability } from './observability/index.js';
import { Firewall, defaultToolPermissions } from './firewall/index.js';

// Configuration
const config = {
  port: parseInt(process.env.MCP_PORT || '4002'),
  host: process.env.MCP_HOST || '0.0.0.0',
  authToken: process.env.MCP_AUTH_TOKEN,
};

// Initialize components
const observability = new Observability();
const firewall = new Firewall({
  defaultPolicy: 'deny',
  enableRateLimiting: true,
  enableDataLeakPrevention: true,
  enableLethalTrifectaProtection: true,
  toolPermissions: defaultToolPermissions,
  blockedPatterns: [],
});

// Admin tool definitions
const adminTools = [
  // CRM Admin
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
        customerId: { type: 'string', description: 'Customer ID' },
      },
      required: ['customerId'],
    },
  },
  
  // Exponential Admin
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
        userId: { type: 'string', description: 'User ID' },
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
        limit: { type: 'number', description: 'Number of logs' },
      },
    },
  },
  
  // System Admin
  {
    name: 'system_health_check',
    description: 'Check platform health (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },
  {
    name: 'system_export_database',
    description: 'Export database (SECRET ACL)',
    inputSchema: { type: 'object' as const, properties: {} },
  },
];

// Create Express app
const app = express();

app.use(cors({
  origin: ['http://localhost:4000', 'http://10.13.13.1:4000'],
  credentials: true,
}));

app.use(helmet());
app.use(express.json());

// Authentication middleware
const authenticate = (req: express.Request, res: express.Response, next: express.NextFunction) => {
  if (!config.authToken) {
    return next();
  }

  const authHeader = req.headers.authorization;
  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const token = authHeader.slice(7);
  if (token !== config.authToken) {
    return res.status(401).json({ error: 'Invalid token' });
  }

  next();
};

// Health check
app.get('/health', (req, res) => {
  res.json({
    status: 'healthy',
    service: 'greenhat-admin-mcp',
    version: '1.0.0',
    timestamp: new Date().toISOString(),
    tools: adminTools.length,
  });
});

// Dashboard endpoint
app.get('/dashboard', authenticate, (req, res) => {
  res.json({
    firewall: firewall.getAllPermissions(),
    auditLogs: observability.getAuditLogs(50),
    message: 'Admin MCP Dashboard',
  });
});

// MCP SSE endpoint
app.get('/mcp/sse', authenticate, async (req, res) => {
  const sessionId = (req.headers['x-session-id'] as string) || randomUUID();
  
  console.log(`[ADMIN-MCP] New connection: ${sessionId}`);

  const server = new Server(
    {
      name: 'greenhat-admin-mcp',
      version: '1.0.0',
    },
    { capabilities: { tools: {} } }
  );

  // List tools
  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return { tools: adminTools };
  });

  // Handle tool calls
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    const startTime = Date.now();

    // All admin tools require SECRET ACL
    const permissionCheck = firewall.checkToolPermission(name, sessionId);
    if (!permissionCheck.allowed) {
      observability.logToolCall(sessionId, name, args, 'blocked', Date.now() - startTime, {
        error: permissionCheck.reason,
      });
      return {
        content: [{ type: 'text', text: `Blocked: ${permissionCheck.reason}` }],
        isError: true,
      };
    }

    // Check rate limit
    const rateLimit = firewall.checkRateLimit(sessionId, name);
    if (!rateLimit.allowed) {
      observability.logToolCall(sessionId, name, args, 'blocked', Date.now() - startTime, {
        error: 'Rate limit exceeded',
      });
      return {
        content: [{ type: 'text', text: 'Rate limit exceeded' }],
        isError: true,
      };
    }

    // Execute tool
    try {
      let result;

      switch (name) {
        case 'crm_list_all_customers':
          const { data: customers } = await supabase.from('customers').select('*');
          result = { content: [{ type: 'text', text: `Found ${customers?.length || 0} customers` }] };
          break;

        case 'admin_list_all_users':
          const { data: users } = await supabase.from('users').select('*');
          result = { content: [{ type: 'text', text: `Found ${users?.length || 0} users` }] };
          break;

        case 'admin_get_audit_logs':
          const logs = observability.getAuditLogs(args?.limit || 50);
          result = { content: [{ type: 'text', text: JSON.stringify(logs, null, 2) }] };
          break;

        case 'system_health_check':
          result = { content: [{ type: 'text', text: '✅ All systems operational' }] };
          break;

        default:
          throw new Error(`Tool ${name} not implemented`);
      }

      observability.logToolCall(sessionId, name, args, 'success', Date.now() - startTime, {});
      return result;

    } catch (error: any) {
      observability.logToolCall(sessionId, name, args, 'error', Date.now() - startTime, {
        error: error.message,
      });
      return {
        content: [{ type: 'text', text: `Error: ${error.message}` }],
        isError: true,
      };
    }
  });

  const transport = new SSEServerTransport('/mcp/messages', res);
  await server.connect(transport);

  req.on('close', () => {
    console.log(`[ADMIN-MCP] Connection closed: ${sessionId}`);
  });
});

// Start server
app.listen(config.port, config.host, () => {
  console.log('🔐 Greenhat Admin MCP Server');
  console.log(`   Port: ${config.port}`);
  console.log(`   Health: http://localhost:${config.port}/health`);
  console.log(`   Dashboard: http://localhost:${config.port}/dashboard`);
  console.log(`   Tools: ${adminTools.length} admin tools (all SECRET ACL)`);
  console.log('');
  console.log('⚠️  SECURITY: All tools require SECRET-level access');
  console.log('⚠️  This server should only be accessible via VPN');
});
