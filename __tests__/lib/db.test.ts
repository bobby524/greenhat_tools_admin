import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock pg Pool before importing the module
const mockQuery = vi.fn();
const mockConnect = vi.fn();
const mockRelease = vi.fn();
const mockEnd = vi.fn();

vi.mock('pg', () => ({
  Pool: vi.fn().mockImplementation(() => ({
    connect: mockConnect,
    query: mockQuery,
    end: mockEnd,
  })),
}));

// Import after mocking
import { getPool, getDatabaseUrl } from '../../lib/db';

describe('Database Utilities', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.resetModules();
    process.env = { ...originalEnv };
    vi.clearAllMocks();
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  describe('getDatabaseUrl', () => {
    it('should return crm_POSTGRES_URL_NON_POOLING if available', () => {
      process.env.crm_POSTGRES_URL_NON_POOLING = 'postgres://crm:pass@localhost/db';
      process.env.POSTGRES_URL = 'postgres://other:pass@localhost/db';
      
      const url = getDatabaseUrl();
      expect(url).toBe('postgres://crm:pass@localhost/db');
    });

    it('should fallback to POSTGRES_URL', () => {
      delete process.env.crm_POSTGRES_URL_NON_POOLING;
      process.env.POSTGRES_URL = 'postgres://other:pass@localhost/db';
      
      const url = getDatabaseUrl();
      expect(url).toBe('postgres://other:pass@localhost/db');
    });

    it('should fallback to DATABASE_URL', () => {
      delete process.env.crm_POSTGRES_URL_NON_POOLING;
      delete process.env.POSTGRES_URL;
      process.env.DATABASE_URL = 'postgres://fallback:pass@localhost/db';
      
      const url = getDatabaseUrl();
      expect(url).toBe('postgres://fallback:pass@localhost/db');
    });

    it('should fallback to CRM_POSTGRES_URL', () => {
      delete process.env.crm_POSTGRES_URL_NON_POOLING;
      delete process.env.POSTGRES_URL;
      delete process.env.DATABASE_URL;
      process.env.CRM_POSTGRES_URL = 'postgres://crm2:pass@localhost/db';
      
      const url = getDatabaseUrl();
      expect(url).toBe('postgres://crm2:pass@localhost/db');
    });

    it('should return null if no database URL is configured', () => {
      delete process.env.crm_POSTGRES_URL_NON_POOLING;
      delete process.env.POSTGRES_URL;
      delete process.env.DATABASE_URL;
      delete process.env.CRM_POSTGRES_URL;
      
      const url = getDatabaseUrl();
      expect(url).toBeNull();
    });
  });

  describe('getPool', () => {
    it('should return null if no database URL is configured', () => {
      delete process.env.crm_POSTGRES_URL_NON_POOLING;
      delete process.env.POSTGRES_URL;
      delete process.env.DATABASE_URL;
      delete process.env.CRM_POSTGRES_URL;
      
      const pool = getPool();
      expect(pool).toBeNull();
    });

    it('should create a Pool with correct configuration for Supabase', () => {
      process.env.crm_POSTGRES_URL_NON_POOLING = 'postgres://user:pass@db.supabase.co:5432/db';
      
      const { Pool } = require('pg');
      const pool = getPool();
      
      expect(Pool).toHaveBeenCalledWith({
        connectionString: 'postgres://user:pass@db.supabase.co:5432/db',
        ssl: { rejectUnauthorized: false },
        max: 5,
        idleTimeoutMillis: 30000,
        connectionTimeoutMillis: 5000,
      });
    });

    it('should create a Pool without SSL for local databases', () => {
      process.env.POSTGRES_URL = 'postgres://user:pass@localhost:5432/db';
      
      const { Pool } = require('pg');
      const pool = getPool();
      
      expect(Pool).toHaveBeenCalledWith({
        connectionString: 'postgres://user:pass@localhost:5432/db',
        ssl: undefined,
        max: 5,
        idleTimeoutMillis: 30000,
        connectionTimeoutMillis: 5000,
      });
    });

    it('should return the same pool instance (singleton pattern)', () => {
      process.env.POSTGRES_URL = 'postgres://user:pass@localhost:5432/db';
      
      const pool1 = getPool();
      const pool2 = getPool();
      
      expect(pool1).toBe(pool2);
    });
  });
});
