import { describe, it, expect, vi, beforeEach } from 'vitest';
import { NextRequest } from 'next/server';

// Mock the database utilities
vi.mock('@/lib/db', () => ({
  getPool: vi.fn(),
}));

import { getPool } from '@/lib/db';
import { GET, PATCH } from '@/app/api/users/route';

describe('Users API', () => {
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
  });

  describe('GET /api/users', () => {
    it('should return 500 if database is not configured', async () => {
      (getPool as any).mockReturnValue(null);
      
      const request = new NextRequest('http://localhost:3000/api/users');
      const response = await GET(request);
      
      expect(response.status).toBe(500);
      const data = await response.json();
      expect(data.error).toBe('Database not configured');
    });

    it('should return list of users', async () => {
      const mockUsers = [
        {
          id: 'user-1',
          email: 'admin@example.com',
          name: 'Admin User',
          image: null,
          emailVerified: true,
          role: 'admin',
          createdAt: '2024-01-01T00:00:00Z',
          updatedAt: '2024-01-01T00:00:00Z',
        },
        {
          id: 'user-2',
          email: 'user@example.com',
          name: 'Regular User',
          image: null,
          emailVerified: false,
          role: 'user',
          createdAt: '2024-01-02T00:00:00Z',
          updatedAt: '2024-01-02T00:00:00Z',
        },
      ];
      
      mockQuery.mockResolvedValue({ rows: mockUsers });
      
      const request = new NextRequest('http://localhost:3000/api/users');
      const response = await GET(request);
      
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.users).toHaveLength(2);
      expect(data.users[0].email).toBe('admin@example.com');
      expect(data.users[1].role).toBe('user');
    });

    it('should handle database errors gracefully', async () => {
      mockQuery.mockRejectedValue(new Error('Connection failed'));
      
      const request = new NextRequest('http://localhost:3000/api/users');
      const response = await GET(request);
      
      expect(response.status).toBe(500);
      const data = await response.json();
      expect(data.error).toBe('Failed to fetch users');
    });

    it('should release client after query', async () => {
      mockQuery.mockResolvedValue({ rows: [] });
      
      const request = new NextRequest('http://localhost:3000/api/users');
      await GET(request);
      
      expect(mockRelease).toHaveBeenCalled();
    });
  });

  describe('PATCH /api/users', () => {
    it('should return 400 if userId is missing', async () => {
      const request = new NextRequest('http://localhost:3000/api/users', {
        method: 'PATCH',
        body: JSON.stringify({ role: 'admin' }),
      });
      
      const response = await PATCH(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('userId');
    });

    it('should return 400 if role is missing', async () => {
      const request = new NextRequest('http://localhost:3000/api/users', {
        method: 'PATCH',
        body: JSON.stringify({ userId: 'user-1' }),
      });
      
      const response = await PATCH(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('role');
    });

    it('should return 400 for invalid role', async () => {
      const request = new NextRequest('http://localhost:3000/api/users', {
        method: 'PATCH',
        body: JSON.stringify({ userId: 'user-1', role: 'superadmin' }),
      });
      
      const response = await PATCH(request);
      
      expect(response.status).toBe(400);
      const data = await response.json();
      expect(data.error).toContain('Invalid role');
    });

    it('should update user role successfully', async () => {
      const updatedUser = {
        id: 'user-1',
        email: 'user@example.com',
        name: 'Test User',
        image: null,
        emailVerified: true,
        role: 'admin',
        createdAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-15T00:00:00Z',
      };
      
      mockQuery.mockResolvedValue({
        rowCount: 1,
        rows: [updatedUser],
      });
      
      const request = new NextRequest('http://localhost:3000/api/users', {
        method: 'PATCH',
        body: JSON.stringify({ userId: 'user-1', role: 'admin' }),
      });
      
      const response = await PATCH(request);
      
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.user.role).toBe('admin');
      expect(data.success).toBe(true);
    });

    it('should return 404 if user not found', async () => {
      mockQuery.mockResolvedValue({
        rowCount: 0,
        rows: [],
      });
      
      const request = new NextRequest('http://localhost:3000/api/users', {
        method: 'PATCH',
        body: JSON.stringify({ userId: 'non-existent', role: 'admin' }),
      });
      
      const response = await PATCH(request);
      
      expect(response.status).toBe(404);
      const data = await response.json();
      expect(data.error).toBe('User not found');
    });
  });
});
