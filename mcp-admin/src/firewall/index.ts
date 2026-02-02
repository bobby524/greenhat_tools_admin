// Admin Firewall - All tools are SECRET level only

export interface ToolPermission {
  enabled: boolean;
  writeOperation: boolean;
  readPrivateData: boolean;
  readUntrustedPublicData: boolean;
  externalCommunication: boolean;
  acl: 'SECRET'; // Admin tools are always SECRET
  rateLimit?: {
    requestsPerMinute: number;
    requestsPerHour: number;
  };
}

export interface FirewallConfig {
  defaultPolicy: 'allow' | 'deny';
  enableRateLimiting: boolean;
  enableDataLeakPrevention: boolean;
  enableLethalTrifectaProtection: boolean;
  toolPermissions: Record<string, ToolPermission>;
  blockedPatterns: string[];
}

// All admin tools are SECRET level
export const defaultToolPermissions: Record<string, ToolPermission> = {
  // CRM Admin Tools
  'crm_list_all_customers': {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
  'crm_delete_customer': {
    enabled: true,
    writeOperation: true,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
  },
  'crm_export_customers': {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
  },

  // Exponential Admin Tools
  'admin_list_all_users': {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
  'admin_delete_user': {
    enabled: true,
    writeOperation: true,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
  },
  'admin_get_audit_logs': {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 30, requestsPerHour: 500 },
  },

  // System Admin Tools
  'system_export_database': {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 2, requestsPerHour: 20 },
  },
  'system_health_check': {
    enabled: true,
    writeOperation: false,
    readPrivateData: false,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: 'SECRET',
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
};

export class Firewall {
  private config: FirewallConfig;
  private blockedSessions: Set<string> = new Set();
  private rateLimits: Map<string, { count: number; resetTime: number }> = new Map();

  constructor(config: Partial<FirewallConfig> = {}) {
    this.config = {
      defaultPolicy: 'deny',
      enableRateLimiting: true,
      enableDataLeakPrevention: true,
      enableLethalTrifectaProtection: true,
      toolPermissions: defaultToolPermissions,
      blockedPatterns: ['drop table', 'delete from', 'rm -rf', 'exec(', 'eval('],
      ...config,
    };
  }

  checkToolPermission(toolName: string, sessionId: string): { allowed: boolean; permission?: ToolPermission; reason?: string } {
    if (this.blockedSessions.has(sessionId)) {
      return { allowed: false, reason: 'Session blocked' };
    }

    const permission = this.config.toolPermissions[toolName];
    if (!permission || !permission.enabled) {
      return { allowed: false, reason: `Tool ${toolName} not found or disabled` };
    }

    // All admin tools require SECRET ACL - no exceptions
    if (permission.acl !== 'SECRET') {
      return { allowed: false, reason: 'Admin tools require SECRET ACL' };
    }

    return { allowed: true, permission };
  }

  checkRateLimit(sessionId: string, toolName: string): { allowed: boolean; resetTime?: number } {
    if (!this.config.enableRateLimiting) {
      return { allowed: true };
    }

    const permission = this.config.toolPermissions[toolName];
    if (!permission?.rateLimit) {
      return { allowed: true };
    }

    const key = `${sessionId}:${toolName}`;
    const now = Date.now();
    const limit = this.rateLimits.get(key);

    if (!limit || now > limit.resetTime) {
      this.rateLimits.set(key, {
        count: 1,
        resetTime: now + 60000, // 1 minute
      });
      return { allowed: true };
    }

    if (limit.count >= permission.rateLimit.requestsPerMinute) {
      return { allowed: false, resetTime: limit.resetTime };
    }

    limit.count++;
    return { allowed: true };
  }

  getAllPermissions(): Record<string, ToolPermission> {
    return { ...this.config.toolPermissions };
  }

  blockSession(sessionId: string): void {
    this.blockedSessions.add(sessionId);
  }
}
