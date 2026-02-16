"use client";

import { useEffect, useState } from "react";
import { authClient } from "@/lib/auth-client";
import { useRouter, usePathname } from "next/navigation";
import Link from "next/link";
import { 
  LayoutDashboard, 
  Shield, 
  Users, 
  Lock, 
  LogOut, 
  Menu,
  X,
  ChevronDown,
  Users2,
  FolderGit
} from "lucide-react";

interface AdminLayoutProps {
  children: React.ReactNode;
  title: string;
}

// Green color palette matching tools.greenhatsec.com
const COLORS = {
  primary: "#62ac4a",
  primaryHover: "#4e8a3a",
  primaryDeep: "#41734a",
  sidebarBg: "#0f2815",
  sidebarSurface: "#1a3d23",
  sidebarHover: "#2d5a3a",
  sidebarBorder: "#2d5a3a",
  textLight: "#f1f5f9",
  textMuted: "#94a3b8",
  bgMain: "#f9fafb",
  cardBg: "#ffffff",
  borderLight: "#e5e7eb",
  borderHover: "#62ac4a",
};

export default function AdminLayout({ children, title }: AdminLayoutProps) {
  const [session, setSession] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [isAdmin, setIsAdmin] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const router = useRouter();
  const pathname = usePathname();

  useEffect(() => {
    checkSession();
  }, []);

  // Close sidebar on route change (mobile)
  useEffect(() => {
    setSidebarOpen(false);
  }, [pathname]);

  async function checkSession() {
    try {
      const { data } = await authClient.getSession();
      setSession(data);
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
      <div className="min-h-screen flex items-center justify-center bg-gray-50">
        <div className="flex items-center gap-3 text-gray-500">
          <div className="w-5 h-5 border-2 border-gray-300 border-t-green-600 rounded-full animate-spin" />
          Loading...
        </div>
      </div>
    );
  }

  if (!session?.user) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-50 p-4">
        <div className="max-w-md w-full bg-white rounded-xl border border-gray-200 p-8 text-center">
          <div className="w-12 h-12 bg-red-100 rounded-xl flex items-center justify-center mx-auto mb-4">
            <Lock className="w-6 h-6 text-red-600" />
          </div>
          <h1 className="text-xl font-semibold text-gray-900 mb-2">Access Denied</h1>
          <p className="text-gray-600 mb-6">Please sign in to access the admin dashboard.</p>
          <button
            onClick={() => authClient.signIn.social({ provider: "google", callbackURL: "/admin" })}
            className="w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium"
          >
            Sign In with Google
          </button>
        </div>
      </div>
    );
  }

  if (!isAdmin) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-50 p-4">
        <div className="max-w-md w-full bg-white rounded-xl border border-gray-200 p-8 text-center">
          <div className="w-12 h-12 bg-red-100 rounded-xl flex items-center justify-center mx-auto mb-4">
            <Lock className="w-6 h-6 text-red-600" />
          </div>
          <h1 className="text-xl font-semibold text-gray-900 mb-2">Access Denied</h1>
          <p className="text-gray-600 mb-2">You do not have permission to access the admin dashboard.</p>
          <p className="text-sm text-gray-500 mb-6">Current role: {session.user.role || "user"}</p>
          <button
            onClick={handleSignOut}
            className="w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-red-600 text-white rounded-lg hover:bg-red-700 transition font-medium"
          >
            <LogOut className="w-4 h-4" />
            Sign Out
          </button>
        </div>
      </div>
    );
  }

  const navItems = [
    { href: "/admin", icon: LayoutDashboard, label: "Dashboard" },
    { href: "/admin/teams", icon: FolderGit, label: "Teams" },
    { href: "/admin/users", icon: Users2, label: "Users" },
    { href: "/admin/mcp-firewall", icon: Shield, label: "MCP Firewall" },
    { href: "/admin/greenspot", icon: Users, label: "Greenspot Admin" },
    { href: "/admin/access-controls", icon: Lock, label: "Access Controls" },
  ];

  return (
    <div className="min-h-screen flex bg-gray-50">
      {/* Mobile Overlay */}
      {sidebarOpen && (
        <div
          className="fixed inset-0 bg-black/50 z-40 lg:hidden"
          onClick={() => setSidebarOpen(false)}
        />
      )}

      {/* Mobile Hamburger Button */}
      <button
        onClick={() => setSidebarOpen(!sidebarOpen)}
        className="fixed top-4 left-4 z-50 p-2 rounded-lg bg-[#0f2815] border border-[#2d5a3a] lg:hidden"
        aria-label="Toggle sidebar"
      >
        {sidebarOpen ? (
          <X className="w-5 h-5 text-gray-200" />
        ) : (
          <Menu className="w-5 h-5 text-gray-200" />
        )}
      </button>

      {/* Sidebar */}
      <aside
        className={`fixed top-0 left-0 z-40 h-full w-64 bg-[#0f2815] border-r border-[#2d5a3a] flex flex-col transition-transform duration-200 lg:translate-x-0 ${
          sidebarOpen ? "translate-x-0" : "-translate-x-full"
        }`}
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-4 py-4 border-b border-[#2d5a3a]">
          <div className="w-8 h-8 rounded-lg bg-[#62ac4a] flex items-center justify-center flex-shrink-0">
            <Shield className="w-4 h-4 text-white" />
          </div>
          <div className="flex-1 min-w-0">
            <h2 className="text-sm font-semibold text-gray-100">Greenhat Admin</h2>
            <p className="text-xs text-gray-400 truncate">{session.user.email}</p>
          </div>
        </div>

        {/* Navigation */}
        <nav className="flex-1 overflow-y-auto px-2 py-3 space-y-1">
          {navItems.map((item) => {
            const isActive = pathname === item.href;
            const Icon = item.icon;
            return (
              <Link
                key={item.href}
                href={item.href}
                className={`flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
                  isActive
                    ? "bg-[#1a3d23] text-white"
                    : "text-gray-400 hover:text-white hover:bg-[#2d5a3a]"
                }`}
              >
                <Icon className={`w-[18px] h-[18px] ${isActive ? "text-[#62ac4a]" : ""}`} />
                <span>{item.label}</span>
              </Link>
            );
          })}
        </nav>

        {/* Bottom Section */}
        <div className="border-t border-[#2d5a3a] p-2">
          <button
            onClick={handleSignOut}
            className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-gray-400 hover:text-white hover:bg-[#2d5a3a] transition-colors text-sm font-medium"
          >
            <LogOut className="w-[18px] h-[18px]" />
            <span>Sign Out</span>
          </button>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 lg:ml-64 min-h-screen bg-gray-50">
        {/* Header */}
        <header className="bg-white border-b border-gray-200 px-6 py-4 lg:px-8">
          <div className="flex items-center justify-between">
            <h1 className="text-xl font-semibold text-gray-900 ml-10 lg:ml-0">{title}</h1>
            <div className="flex items-center gap-3">
              <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold bg-[#62ac4a]/10 text-[#41734a] border border-[#62ac4a]/20">
                <span className="w-1.5 h-1.5 rounded-full bg-[#62ac4a]" />
                ADMIN
              </span>
            </div>
          </div>
        </header>

        {/* Content */}
        <div className="p-4 lg:p-8">
          {children}
        </div>
      </main>
    </div>
  );
}
