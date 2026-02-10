import { describe, it, expect, vi } from 'vitest';
import { NextRequest } from 'next/server';

// Import the route handler
import { GET } from '@/app/api/health/route';

describe('Health API', () => {
  it('should return healthy status', async () => {
    const request = new NextRequest('http://localhost:3000/api/health');
    const response = await GET();
    
    expect(response.status).toBe(200);
    
    const data = await response.json();
    expect(data.status).toBe('healthy');
    expect(data.service).toBe('greenhat-admin');
    expect(data.version).toBe('1.0.0');
    expect(data.timestamp).toBeDefined();
    
    // Verify timestamp is a valid ISO date
    const timestamp = new Date(data.timestamp);
    expect(timestamp.toISOString()).toBe(data.timestamp);
  });

  it('should return valid JSON content type', async () => {
    const response = await GET();
    
    expect(response.headers.get('content-type')).toContain('application/json');
  });
});
