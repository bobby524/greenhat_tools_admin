import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock Resend before importing
const mockSend = vi.fn();
vi.mock('resend', () => ({
  Resend: vi.fn().mockImplementation(() => ({
    emails: { send: mockSend },
  })),
}));

import { sendInviteEmail, InviteEmailData } from '@/lib/email';

describe('Email Utilities', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    process.env = {
      ...originalEnv,
      RESEND_API_KEY: 'test-api-key',
      RESEND_FROM_EMAIL: 'test@example.com',
      BETTER_AUTH_URL: 'https://admin.example.com',
    };
  });

  describe('sendInviteEmail', () => {
    it('should send invite email with correct parameters', async () => {
      mockSend.mockResolvedValue({ data: { id: 'email-id' } });
      
      const inviteData: InviteEmailData = {
        email: 'newuser@example.com',
        token: 'invite-token-123',
        role: 'admin',
        invitedByName: 'John Doe',
      };
      
      const result = await sendInviteEmail(inviteData);
      
      expect(result.success).toBe(true);
      expect(mockSend).toHaveBeenCalledWith({
        from: 'Greenhat Tools <test@example.com>',
        to: 'newuser@example.com',
        subject: "You've been invited to Greenhat Tools",
        html: expect.stringContaining('John Doe'),
      });
    });

    it('should include correct invite URL in email', async () => {
      mockSend.mockResolvedValue({ data: { id: 'email-id' } });
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'abc123',
        role: 'user',
      };
      
      await sendInviteEmail(inviteData);
      
      const callArgs = mockSend.mock.calls[0][0];
      expect(callArgs.html).toContain('https://admin.example.com/invite?token=abc123');
    });

    it('should handle missing invitedByName gracefully', async () => {
      mockSend.mockResolvedValue({ data: { id: 'email-id' } });
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'user',
        invitedByName: null,
      };
      
      await sendInviteEmail(inviteData);
      
      const callArgs = mockSend.mock.calls[0][0];
      expect(callArgs.html).toContain('Someone');
    });

    it('should capitalize role name in email', async () => {
      mockSend.mockResolvedValue({ data: { id: 'email-id' } });
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'viewer',
      };
      
      await sendInviteEmail(inviteData);
      
      const callArgs = mockSend.mock.calls[0][0];
      expect(callArgs.html).toContain('Viewer Access');
    });

    it('should return error if Resend API fails', async () => {
      mockSend.mockResolvedValue({ 
        error: { message: 'Invalid API key' } 
      });
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'user',
      };
      
      const result = await sendInviteEmail(inviteData);
      
      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid API key');
    });

    it('should return error on exception', async () => {
      mockSend.mockRejectedValue(new Error('Network error'));
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'user',
      };
      
      const result = await sendInviteEmail(inviteData);
      
      expect(result.success).toBe(false);
      expect(result.error).toBe('Network error');
    });

    it('should throw error if RESEND_API_KEY is not set', async () => {
      delete process.env.RESEND_API_KEY;
      
      // Need to re-import to get fresh instance
      vi.resetModules();
      const { sendInviteEmail: freshSendInviteEmail } = await import('@/lib/email');
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'user',
      };
      
      await expect(freshSendInviteEmail(inviteData)).rejects.toThrow('RESEND_API_KEY');
    });

    it('should use default from email if RESEND_FROM_EMAIL not set', async () => {
      delete process.env.RESEND_FROM_EMAIL;
      process.env.BETTER_AUTH_URL = 'https://admin.example.com';
      
      mockSend.mockResolvedValue({ data: { id: 'email-id' } });
      
      const inviteData: InviteEmailData = {
        email: 'user@example.com',
        token: 'token123',
        role: 'user',
      };
      
      await sendInviteEmail(inviteData);
      
      const callArgs = mockSend.mock.calls[0][0];
      expect(callArgs.from).toBe('Greenhat Tools <noreply@greenhatsec.com>');
    });
  });
});
