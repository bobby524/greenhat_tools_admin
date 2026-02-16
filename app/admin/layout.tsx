import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { auth } from "@/lib/auth";

export const metadata = {
  title: "Admin Dashboard",
  description: "Greenhat Tools Admin Portal",
};

function hostAllowed(host: string | null) {
  if (!host) return false;
  const h = host.split(":")[0];

  // Local dev / preview
  if (h === "localhost" || h === "127.0.0.1") return true;

  // Production
  if (h === "admin.greenhatsec.com") return true;

  // Vercel preview domains (optional)
  if (h.endsWith(".vercel.app")) return true;

  return false;
}

function isAdminUser(user: any) {
  const role = user?.role;
  const email = user?.email;
  const id = user?.id;

  // BetterAuth admin plugin sets role=admin for adminUserIds.
  // Keep a small allowlist fallback (belt + suspenders).
  return (
    role === "admin" ||
    role === "owner" ||
    id === "09649c79-975a-4967-9299-440b2b0fadee" ||
    email === "anthony@greenhatsec.com"
  );
}

export default async function AdminRootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const hdrs = await headers();

  const host = hdrs.get("host");
  if (process.env.NODE_ENV === "production" && !hostAllowed(host)) {
    // Fail closed: admin UI should only be reachable from admin.greenhatsec.com.
    redirect("/");
  }

  // Auth gate (server-side, no flash)
  const session = await (auth.api as any).getSession({ headers: hdrs });
  const user = session?.user;

  if (!user) {
    redirect("/auth/signin?callback=/admin");
  }

  if (!isAdminUser(user)) {
    redirect("/auth/denied");
  }

  return children;
}
