"use client";

import { useSearchParams } from "next/navigation";
import { authClient } from "@/lib/auth-client";

export default function SignInPage() {
  const searchParams = useSearchParams();
  const callback = searchParams.get("callback") || "/admin";

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 p-4">
      <div className="max-w-md w-full bg-white rounded-xl border border-gray-200 p-8 text-center">
        <h1 className="text-xl font-semibold text-gray-900 mb-2">Sign in</h1>
        <p className="text-gray-600 mb-6">You need to sign in to access the admin portal.</p>
        <button
          onClick={() => authClient.signIn.social({ provider: "google", callbackURL: callback })}
          className="w-full inline-flex items-center justify-center gap-2 px-4 py-2.5 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium"
        >
          Sign In with Google
        </button>
      </div>
    </div>
  );
}
