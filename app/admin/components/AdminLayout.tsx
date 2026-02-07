"use client";

import { useEffect, useState } from "react";
import { authClient } from "@/lib/auth-client";
import { useRouter } from "next/navigation";

interface AdminLayoutProps {
  children: React.ReactNode;
  title: string;
}

export default function AdminLayout({ children, title }: AdminLayoutProps) {
  const [session, setSession] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [isAdmin, setIsAdmin] = useState(false);
  const router = useRouter();

  useEffect(() => {
    checkSession();
  }, []);

  async function checkSession() {
    try {
      const { data } = await authClient.getSession();
      setSession(data);
      // Check if user is admin (Better Auth stores role in user object or check by ID/email)
      const userRole = (data?.user as any)?.role;
      const userEmail = data?.user?.email;
      setIsAdmin(
        userRole === "admin" || 
        data?.user?.id === "09649c79-975a-4967-9299-440b2b0fadee" ||
        userEmail === "anthony@greenhatsec.com"
      );
    } catch (e) {
      console.log("No session");
    }
    setLoading(false);
  }

  async function handleSignOut() {
    await authClient.signOut();
    router.push("/");
  }

  if (loading) {
    return (
      <div style={{ padding: 40, fontFamily: "Arial, sans-serif" }}>
        Loading...
      </div>
    );
  }

  if (!session?.user) {
    return (
      <div style={{ padding: 40, fontFamily: "Arial, sans-serif" }}>
        <h1>Access Denied</h1>
        <p>Please sign in to access the admin dashboard.</p>
        <button
          onClick={() => authClient.signIn.social({ provider: "google", callbackURL: "/admin" })}
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
    );
  }

  if (!isAdmin) {
    return (
      <div style={{ padding: 40, fontFamily: "Arial, sans-serif" }}>
        <h1>Access Denied</h1>
        <p>You do not have permission to access the admin dashboard.</p>
        <p>Current role: {session.user.role || "user"}</p>
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
    );
  }

  return (
    <div style={{ display: "flex", minHeight: "100vh", fontFamily: "Arial, sans-serif" }}>
      {/* Sidebar */}
      <aside
        style={{
          width: "260px",
          background: "#1a1a2e",
          color: "white",
          padding: "20px",
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div style={{ marginBottom: "30px" }}>
          <h2 style={{ margin: 0, fontSize: "1.25rem" }}>Greenhat Admin</h2>
          <p style={{ margin: "5px 0 0", fontSize: "0.875rem", opacity: 0.7 }}>
            {session.user.email}
          </p>
        </div>

        <nav style={{ flex: 1 }}>
          <NavLink href="/admin" icon="🏠">Dashboard</NavLink>
          <NavLink href="/admin/mcp-firewall" icon="🛡️">MCP Firewall</NavLink>
          <NavLink href="/admin/crm" icon="👥">CRM Admin</NavLink>
          <NavLink href="/admin/access-controls" icon="🔐">Access Controls</NavLink>
        </nav>

        <div style={{ marginTop: "auto", paddingTop: "20px", borderTop: "1px solid #333" }}>
          <button
            onClick={handleSignOut}
            style={{
              width: "100%",
              padding: "10px",
              background: "transparent",
              color: "white",
              border: "1px solid #444",
              borderRadius: "6px",
              cursor: "pointer",
              textAlign: "left",
            }}
          >
            🚪 Sign Out
          </button>
        </div>
      </aside>

      {/* Main Content */}
      <main style={{ flex: 1, background: "#f5f5f5", overflow: "auto" }}>
        <header
          style={{
            background: "white",
            padding: "20px 30px",
            borderBottom: "1px solid #e0e0e0",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <h1 style={{ margin: 0, fontSize: "1.5rem" }}>{title}</h1>
          <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
            <span
              style={{
                background: "#10b981",
                color: "white",
                padding: "4px 12px",
                borderRadius: "12px",
                fontSize: "0.75rem",
                fontWeight: "bold",
              }}
            >
              ADMIN
            </span>
          </div>
        </header>

        <div style={{ padding: "30px" }}>{children}</div>
      </main>
    </div>
  );
}

function NavLink({
  href,
  icon,
  children,
}: {
  href: string;
  icon: string;
  children: React.ReactNode;
}) {
  const router = useRouter();
  const isActive = typeof window !== "undefined" && window.location.pathname === href;

  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        router.push(href);
      }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: "10px",
        padding: "12px 16px",
        margin: "4px 0",
        borderRadius: "8px",
        textDecoration: "none",
        color: isActive ? "white" : "rgba(255,255,255,0.7)",
        background: isActive ? "#2563eb" : "transparent",
        transition: "all 0.2s",
      }}
    >
      <span>{icon}</span>
      <span>{children}</span>
    </a>
  );
}
