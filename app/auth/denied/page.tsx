"use client";

import Link from "next/link";
import { authClient } from "@/lib/auth-client";

export default function AccessDeniedPage() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 p-4">
      <div className="max-w-md w-full bg-white rounded-xl border border-gray-200 p-8 text-center">
        <h1 className="text-xl font-semibold text-gray-900 mb-2">Access denied</h1>
        <p className="text-gray-600 mb-6">Your account doesn’t have permission to access the admin portal.</p>
        <div className="flex flex-col gap-3">
          <button
            onClick={() => authClient.signOut().then(() => (window.location.href = "/"))}
            className="w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-red-600 text-white rounded-lg hover:bg-red-700 transition font-medium"
          >
            Sign out
          </button>
          <Link
            href="/"
            className="w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-gray-100 text-gray-900 rounded-lg hover:bg-gray-200 transition font-medium"
          >
            Go home
          </Link>
        </div>
      </div>
    </div>
  );
}
