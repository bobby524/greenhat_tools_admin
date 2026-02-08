import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';

// Mock the auth client
vi.mock('@/lib/auth-client', () => ({
  authClient: {
    getSession: vi.fn(),
    signOut: vi.fn(),
    signIn: {
      social: vi.fn(),
    },
  },
}));

import { authClient } from '@/lib/auth-client';
import AdminLayout from '@/app/admin/components/AdminLayout';

describe('AdminLayout Component', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should show loading state initially', () => {
    (authClient.getSession as any).mockReturnValue(new Promise(() => {})); // Never resolves
    
    render(
      <AdminLayout title="Test">
        <div>Content</div>
      </AdminLayout>
    );
    
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('should show access denied for unauthenticated users', async () => {
    (authClient.getSession as any).mockResolvedValue({ data: null });
    
    render(
      <AdminLayout title="Test">
        <div>Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByText('Access Denied')).toBeInTheDocument();
    });
    
    expect(screen.getByText('Please sign in to access the admin dashboard.')).toBeInTheDocument();
  });

  it('should show access denied for non-admin users', async () => {
    (authClient.getSession as any).mockResolvedValue({
      data: {
        user: {
          id: 'user-123',
          email: 'user@example.com',
          role: 'user',
        },
      },
    });
    
    render(
      <AdminLayout title="Test">
        <div>Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByText('Access Denied')).toBeInTheDocument();
    });
    
    expect(screen.getByText(/do not have permission/i)).toBeInTheDocument();
  });

  it('should show content for admin users', async () => {
    (authClient.getSession as any).mockResolvedValue({
      data: {
        user: {
          id: 'admin-123',
          email: 'admin@example.com',
          role: 'admin',
        },
      },
    });
    
    render(
      <AdminLayout title="Test Dashboard">
        <div data-testid="content">Admin Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByTestId('content')).toBeInTheDocument();
    });
    
    expect(screen.getByText('Test Dashboard')).toBeInTheDocument();
    expect(screen.getByText('ADMIN')).toBeInTheDocument();
  });

  it('should show content for hardcoded admin user', async () => {
    (authClient.getSession as any).mockResolvedValue({
      data: {
        user: {
          id: '09649c79-975a-4967-9299-440b2b0fadee', // Hardcoded admin ID
          email: 'special@example.com',
          role: 'user', // Even with user role, this ID is admin
        },
      },
    });
    
    render(
      <AdminLayout title="Test">
        <div data-testid="content">Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByTestId('content')).toBeInTheDocument();
    });
  });

  it('should call signOut when sign out button clicked', async () => {
    (authClient.getSession as any).mockResolvedValue({
      data: {
        user: {
          id: 'admin-123',
          email: 'admin@example.com',
          role: 'admin',
        },
      },
    });
    
    render(
      <AdminLayout title="Test">
        <div>Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByText('Sign Out')).toBeInTheDocument();
    });
    
    fireEvent.click(screen.getByText('Sign Out'));
    
    expect(authClient.signOut).toHaveBeenCalled();
  });

  it('should render navigation items', async () => {
    (authClient.getSession as any).mockResolvedValue({
      data: {
        user: {
          id: 'admin-123',
          email: 'admin@example.com',
          role: 'admin',
        },
      },
    });
    
    render(
      <AdminLayout title="Test">
        <div>Content</div>
      </AdminLayout>
    );
    
    await waitFor(() => {
      expect(screen.getByText('Dashboard')).toBeInTheDocument();
    });
    
    expect(screen.getByText('MCP Firewall')).toBeInTheDocument();
    expect(screen.getByText('GreenSpot')).toBeInTheDocument();
    expect(screen.getByText('Access Controls')).toBeInTheDocument();
  });
});
