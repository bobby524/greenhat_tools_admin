"use client";
<<<<<<< HEAD

import { useEffect, useState } from "react";
import { authClient } from "@/lib/auth-client";
import { Chrome, Loader2 } from "lucide-react";

export default function AdminDashboard() {
  const [session, setSession] = useState<any>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    authClient.getSession().then(({ data }) => {
      setSession(data);
      setIsLoading(false);
    });
  }, []);

  const handleSignIn = async () => {
    await authClient.signIn.social({
      provider: "google",
    });
  };

  const handleSignOut = async () => {
    await authClient.signOut();
    window.location.href = "/";
  };

  if (isLoading) {
    return (
      <div style={{ padding: '40px', display: 'flex', justifyContent: 'center' }}>
        <Loader2 className="w-8 h-8 animate-spin" />
      </div>
    );
  }

  // Not authenticated
  if (!session?.user) {
    return (
      <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
          <h1>🛡️ Greenhat Tools Admin</h1>
        </div>
        
        <div style={{ 
          padding: '40px', 
          background: '#f5f5f5', 
          borderRadius: '8px',
          textAlign: 'center'
        }}>
          <h2>Admin Access Required</h2>
          <p>Please sign in to access the admin dashboard.</p>
          <button 
            onClick={handleSignIn}
            style={{
              padding: '12px 24px',
              background: '#1a1a2e',
              color: 'white',
              border: 'none',
              borderRadius: '6px',
              cursor: 'pointer',
              fontSize: '16px',
              marginTop: '16px',
              display: 'inline-flex',
              alignItems: 'center',
              gap: '8px'
            }}
          >
            <Chrome className="w-5 h-5" />
            Sign in with Google
          </button>
        </div>
      </div>
    );
  }

  // Authenticated - show dashboard
  return (
    <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        <h1>🛡️ Greenhat Tools Admin</h1>
        <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
          <span>{session.user.email}</span>
          <button 
            onClick={handleSignOut}
            style={{
              padding: '8px 16px',
              background: '#dc2626',
              color: 'white',
              border: 'none',
              borderRadius: '6px',
              cursor: 'pointer',
            }}
          >
            Sign out
          </button>
        </div>
      </div>
      
      <p>Platform-wide administrative interface</p>
      
      <div style={{ 
        display: 'grid', 
        gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))',
        gap: '20px',
        marginTop: '40px'
      }}>
        <AdminCard 
          title="📊 CRM Admin"
          description="Manage customers, deals, pipelines"
          href="/admin/crm"
        />
        <AdminCard 
          title="🎫 Exponential Admin"
          description="User management, boards, security"
          href="/admin/exponential"
        />
        <AdminCard 
          title="🔒 SOC2 Admin"
          description="Compliance, audit trails"
          href="/admin/soc2"
        />
        <AdminCard 
          title="⚙️ System Admin"
          description="Global settings, backups"
          href="/admin/system"
        />
        <AdminCard 
          title="📜 Audit Logs"
          description="Platform-wide activity logs"
          href="/admin/audit"
        />
        <AdminCard 
          title="🛡️ MCP Firewall"
          description="Manage tool permissions and security policies"
          href="/admin/firewall"
        />
        <AdminCard 
          title="👤 User Access"
          description="Manage user roles, permissions, and access control"
          href="/admin/users"
        />
      </div>

      <div style={{ marginTop: '40px', padding: '20px', background: '#f5f5f5', borderRadius: '8px' }}>
        <h3>🔐 Security Status</h3>
        <ul>
          <li>✅ Authentication Required</li>
          <li>✅ All Actions Logged</li>
          <li>✅ SECRET-Level Tools</li>
          <li>✅ Role-Based Access Control</li>
        </ul>
      </div>
    </div>
  );
}

function AdminCard({ title, description, href }: { 
  title: string
  description: string
  href: string 
}) {
  return (
    <a 
      href={href}
      style={{
        display: 'block',
        padding: '24px',
        border: '1px solid #ddd',
        borderRadius: '8px',
        textDecoration: 'none',
        color: 'inherit',
        transition: 'box-shadow 0.2s',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0,0,0,0.1)'
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.boxShadow = 'none'
      }}
    >
      <h3 style={{ margin: '0 0 8px 0' }}>{title}</h3>
      <p style={{ margin: 0, color: '#666' }}>{description}</p>
    </a>
  )
}
=======

import { useEffect, useState } from "react";
import { authClient } from "@/lib/auth-client";

export default function Home() {
  const [session, setSession] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    checkSession();
  }, []);

  async function checkSession() {
    try {
      const { data } = await authClient.getSession();
      setSession(data);
    } catch (e) {
      console.log("No session");
    }
    setLoading(false);
  }

  async function handleSignIn() {
    await authClient.signIn.social({
      provider: "google",
      callbackURL: "/",
    });
  }

  async function handleSignOut() {
    await authClient.signOut();
    setSession(null);
  }

  if (loading) {
    return <div style={{ padding: 40 }}>Loading...</div>;
  }

  return (
    <div style={{ padding: "40px", fontFamily: "Arial, sans-serif", maxWidth: 800 }}>
      <h1>Greenhat Tools Admin MCP Server</h1>
      <p>API Gateway for admin tools and MCP firewall.</p>
      <p>MCP Endpoint: <code>/api/mcp</code></p>
      
      <hr style={{ margin: "30px 0" }} />
      
      <h2>Authentication</h2>
      
      {session?.user ? (
        <div>
          <p>✅ Logged in as: <strong>{session.user.email}</strong></p>
          <p>Name: {session.user.name}</p>
          <p>Role: {session.user.role || "user"}</p>
          <button 
            onClick={handleSignOut}
            style={{
              padding: "10px 20px",
              background: "#dc2626",
              color: "white",
              border: "none",
              borderRadius: "6px",
              cursor: "pointer",
              marginTop: 10,
            }}
          >
            Sign Out
          </button>
        </div>
      ) : (
        <div>
          <p>❌ Not logged in</p>
          <button 
            onClick={handleSignIn}
            style={{
              padding: "10px 20px",
              background: "#2563eb",
              color: "white",
              border: "none",
              borderRadius: "6px",
              cursor: "pointer",
              marginTop: 10,
            }}
          >
            Sign In with Google
          </button>
        </div>
      )}
      
      <hr style={{ margin: "30px 0" }} />
      
      <h2>Test Shared Auth</h2>
      <p>If you login on tools.greenhatsec.com, you should see your session here automatically!</p>
      <p>
        <a href="https://tools.greenhatsec.com" target="_blank" rel="noopener noreferrer">
          Open tools.greenhatsec.com →
        </a>
      </p>
    </div>
  );
}
// Redeploy trigger: Sat Feb  7 01:32:25 PST 2026
// Redeploy: Sat Feb  7 01:58:17 PST 2026
// Deploy after env vars added: Sat Feb  7 06:46:46 PST 2026
// Force fresh deploy: Sat Feb  7 06:49:35 PST 2026
// Redeploy with GH secrets: Sat Feb  7 06:55:09 PST 2026
// Redeploy: Sat Feb  7 07:11:01 PST 2026
// Force redeploy: Sat Feb  7 11:53:02 PST 2026
>>>>>>> fix-auth-deploy
