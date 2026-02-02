// Audit logging for admin operations

export interface AuditLog {
  timestamp: string;
  sessionId: string;
  adminUser: string;
  tool: string;
  params: any;
  result: 'success' | 'error' | 'blocked';
  error?: string;
  duration: number;
  ip?: string;
  acl: 'SECRET';
}

export class Observability {
  private logs: AuditLog[] = [];
  private maxLogs = 10000;

  logToolCall(
    sessionId: string,
    tool: string,
    params: any,
    result: 'success' | 'error' | 'blocked',
    duration: number,
    meta: { error?: string; ip?: string; adminUser?: string }
  ): void {
    const log: AuditLog = {
      timestamp: new Date().toISOString(),
      sessionId,
      adminUser: meta.adminUser || 'unknown',
      tool,
      params: this.sanitizeParams(params),
      result,
      error: meta.error,
      duration,
      ip: meta.ip,
      acl: 'SECRET',
    };

    this.logs.unshift(log);
    
    // Keep only last N logs
    if (this.logs.length > this.maxLogs) {
      this.logs = this.logs.slice(0, this.maxLogs);
    }

    // Also log to console/file
    console.log(`[AUDIT] ${log.timestamp} | ${log.adminUser} | ${tool} | ${result} | ${duration}ms`);
  }

  getAuditLogs(limit = 100): AuditLog[] {
    return this.logs.slice(0, limit);
  }

  private sanitizeParams(params: any): any {
    // Remove sensitive data from logs
    if (typeof params !== 'object' || params === null) {
      return params;
    }

    const sanitized = { ...params };
    const sensitiveKeys = ['password', 'token', 'key', 'secret', 'credit_card'];
    
    for (const key of Object.keys(sanitized)) {
      if (sensitiveKeys.some(sk => key.toLowerCase().includes(sk))) {
        sanitized[key] = '***REDACTED***';
      }
    }

    return sanitized;
  }
}
