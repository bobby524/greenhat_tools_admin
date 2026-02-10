<<<<<<< HEAD
"use client";

import { useEffect, useState } from "react";
import { authClient } from "@/lib/auth-client";

export default function AdminLayout({
=======
export const metadata = {
  title: "Admin Dashboard",
  description: "Greenhat Tools Admin Portal",
};

export default function AdminRootLayout({
>>>>>>> fix-auth-deploy
  children,
}: {
  children: React.ReactNode;
}) {
<<<<<<< HEAD
  const [user, setUser] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    authClient.getSession().then(({ data }) => {
      setUser(data?.user || null);
      setLoading(false);
    });
  }, []);

  const handleSignOut = async () => {
    await authClient.signOut();
    window.location.href = "/";
  };

  if (loading) {
    return <div>Loading...</div>;
  }

  return (
    <div>
      <nav style={{
        background: '#1a1a2e',
        color: 'white',
        padding: '16px 40px',
        display: 'flex',
        gap: '24px',
        alignItems: 'center',
        justifyContent: 'space-between'
      }}>
        <div style={{ display: 'flex', gap: '24px', alignItems: 'center' }}>
          <a href="/admin" style={{ color: 'white', textDecoration: 'none', fontWeight: 'bold' }}>
            🛡️ Admin
          </a>
          <a href="/admin/crm" style={{ color: '#ccc', textDecoration: 'none' }}>CRM</a>
          <a href="/admin/exponential" style={{ color: '#ccc', textDecoration: 'none' }}>Exponential</a>
          <a href="/admin/soc2" style={{ color: '#ccc', textDecoration: 'none' }}>SOC2</a>
          <a href="/admin/system" style={{ color: '#ccc', textDecoration: 'none' }}>System</a>
          <a href="/admin/audit" style={{ color: '#ccc', textDecoration: 'none' }}>Audit</a>
          <a href="/admin/firewall" style={{ color: '#ccc', textDecoration: 'none' }}>🛡️ Firewall</a>
          <a href="/admin/users" style={{ color: '#ccc', textDecoration: 'none' }}>👤 User Access</a>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <span style={{ color: '#ccc', fontSize: '14px' }}>
            {user?.email}
          </span>
          <button 
            onClick={handleSignOut}
            style={{
              background: '#dc2626',
              color: 'white',
              border: 'none',
              padding: '8px 16px',
              borderRadius: '6px',
              cursor: 'pointer',
              fontSize: '14px'
            }}
          >
            Sign out
          </button>
        </div>
      </nav>
      {children}
    </div>
  )
}
=======
  return children;
}
>>>>>>> fix-auth-deploy
