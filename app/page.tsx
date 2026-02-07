"use client";

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
