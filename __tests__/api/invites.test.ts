import { describe, it, expect, vi, beforeEach } from 'vitest';
import { NextRequest } from 'next/server';

// Mock dependencies
vi.mock('@/lib/db', () => ({
  getPool: vi.fn(),
}));

vi.mock('@/lib/email', () => ({
  sendInviteEmail: vi.fn(),
}));

import { getPool } from '@/lib/db';
import { sendInviteEmail } from '@/lib/email';
import { GET, POST, DELETE } from '@/app/api/invites/route';

describe('Invites API', () => {
  const mockQuery = vi.fn();
  const mockConnect = vi.fn();
  const mockRelease = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    
    mockConnect.mockResolvedValue({
      query: mockQuery,
      release: mockRelease,
    });
    
    (getPool as any).mockReturnValue({
      connect: mockConnect,
    });
    
    // Mock admin session by default
    mockQuery.mockImplementation((sql: string, params: any[]) => {
      if (sql.includes('FROM "session"')) {
        return {
          rows: [{
            id: 'admin-user-id',
            email: 'admin@example.com',
            role: 'admin',
            name: 'Admin User',
          }],
        };
      }
      if (sql.includes('FROM "user"')) {
        return { rows: [] }; // No existing user
      }
      return { rows: [] };
    });
  });

  describe('GET /api/invites', () => {
    it('should return 403 for non-admin users', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'user-id',
              email: 'user@example.com',
              role: 'user',
              name: 'Regular User',
            }],
          };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        headers: {
          cookie: 'greenhat_tools.session_token=test-token',
        },
      });
      
      const response = await GET(request);
      
      expect(response.status).toBe(403);
      const data = await response.json();
      expect(data.error).toContain('Unauthorized');
    });

    it('should return list of invites for admin', async () => {
      const mockInvites = [
        {
          id: 'invite-1',
          email: 'newuser@example.com',
          token: 'token-123',
          role: 'user',
          invitedBy: 'admin-id',
          invitedByName: 'Admin User',
          invitedByEmail: 'admin@example.com',
          expiresAt: '2024-02-01T00:00:00Z',
          usedAt: null,
          createdAt: '2024-01-01T00:00:00Z',
        },
      ];
      
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('FROM invites')) {
          return { rows: mockInvites };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        headers: {
          cookie: 'greenhat_tools.session_token=test-token',
        },
      });
      
      const response = await GET(request);
      
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.invites).toHaveLength(1);
      expect(data.invites[0].status).toBe('pending');
    });

    it('should calculate correct status for invites', async () => {
      const mockInvites = [
        {
          id: 'invite-1',
          email: 'used@example.com',
          token: 'token-1',
          role: 'user',
          invitedBy: 'admin-id',
          invitedByName: null,
          invitedByEmail: 'admin@example.com',
          expiresAt: '2024-02-01T00:00:00Z',
          usedAt: '2024-01-15T00:00:00Z', // Used
          createdAt: '2024-01-01T00:00:00Z',
        },
        {
          id: 'invite-2',
          email: 'expired@example.com',
          token: 'token-2',
          role: 'user',
          invitedBy: 'admin-id',
          invitedByName: null,
          invitedByEmail: 'admin@example.com',
          expiresAt: '2020-01-01T00:00:00Z', // Expired
          usedAt: null,
          createdAt: '2024-01-01T00:00:00Z',
        },
      ];
      
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('FROM invites')) {
          return { rows: mockInvites };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        headers: {
          cookie: 'greenhat_tools.session_token=test-token',
        },
      });
      
      const response = await GET(request);
      const data = await response.json();
      
      expect(data.invites[0].status).toBe('used');
      expect(data.invites[1].status).toBe('expired');
    });
  });

  describe('POST /api/invites', () => {
    it('should return 400 if email is missing', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        method: 'POST',
        body: JSON.stringify({ role: 'user' }),
      });
      
      const response = await POST(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('Email is required');
    });

    it('should return 400 for invalid role', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        method: 'POST',
        body: JSON.stringify({ email: 'test@example.com', role: 'superadmin' }),
      });
      
      const response = await POST(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('Invalid role');
    });

    it('should return 409 if user already exists', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('FROM "user" WHERE email')) {
          return { rows: [{ id: 'existing-user' }] }; // User exists
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        method: 'POST',
        body: JSON.stringify({ email: 'existing@example.com', role: 'user' }),
      });
      
      const response = await POST(request);
      
      expect(response.status).toBe(409);
      const data = await response.json();
      expect(data.error).toContain('already exists');
    });

    it('should create invite successfully', async () => {
      const newInvite = {
        id: 'new-invite-id',
        email: 'newuser@example.com',
        token: 'uuid-token',
        role: 'user',
        expiresAt: '2024-02-01T00:00:00Z',
        createdAt: '2024-01-01T00:00:00Z',
      };
      
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('INSERT INTO invites')) {
          return { rows: [newInvite] };
        }
        return { rows: [] };
      });
      
      (sendInviteEmail as any).mockResolvedValue({ success: true });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        method: 'POST',
        body: JSON.stringify({ email: 'newuser@example.com', role: 'user' }),
      });
      
      const response = await POST(request);
      
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.success).toBe(true);
      expect(data.invite.email).toBe('newuser@example.com');
      expect(sendInviteEmail).toHaveBeenCalled();
    });
  });

  describe('DELETE /api/invites', () => {
    it('should return 400 if invite ID is missing', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites', {
        method: 'DELETE',
      });
      
      const response = await DELETE(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('Invite ID is required');
    });

    it('should return 404 if invite not found or already used', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('DELETE FROM invites')) {
          return { rowCount: 0 }; // No rows deleted
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites?id=non-existent', {
        method: 'DELETE',
      });
      
      const response = await DELETE(request);
      
      expect(response.status).toBe(404);
      const data = await response.json();
      expect(data.error).toContain('not found or already used');
    });

    it('should revoke invite successfully', async () => {
      mockQuery.mockImplementation((sql: string) => {
        if (sql.includes('FROM "session"')) {
          return {
            rows: [{
              id: 'admin-id',
              email: 'admin@example.com',
              role: 'admin',
              name: 'Admin User',
            }],
          };
        }
        if (sql.includes('DELETE FROM invites')) {
          return { rowCount: 1 }; // Successfully deleted
        }
        return { rows: [] };
      });
      
      const request = new NextRequest('http://localhost:3000/api/invites?id=invite-to-revoke', {
        method: 'DELETE',
      });
      
      const response = await DELETE(request);
      
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.success).toBe(true);
    });
  });
});
