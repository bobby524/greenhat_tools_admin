"use client";

import { useEffect } from "react";

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    // Log error to monitoring service
    console.error("Global error boundary caught:", error);
  }, [error]);

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 px-4">
      <div className="max-w-md w-full space-y-6 text-center">
        <div className="space-y-2">
          <h1 className="text-3xl font-bold text-gray-900">Something went wrong</h1>
          <p className="text-gray-600">
            An unexpected error occurred. Please try again or contact support if the problem persists.
          </p>
        </div>
        
        {error.message && (
          <div className="rounded-lg bg-red-50 border border-red-200 p-4 text-left">
            <p className="text-sm font-medium text-red-800">Error details:</p>
            <p className="text-sm text-red-600 mt-1 font-mono break-all">{error.message}</p>
          </div>
        )}

        <div className="flex gap-3 justify-center">
          <button
            onClick={reset}
            className="rounded-full bg-[#62ac4a] px-6 py-3 text-sm font-semibold text-white transition hover:bg-[#41734a]"
          >
            Try again
          </button>
          <a
            href="/"
            className="rounded-full border border-gray-300 bg-white px-6 py-3 text-sm font-semibold text-gray-700 transition hover:border-[#62ac4a] hover:text-[#62ac4a]"
          >
            Go home
          </a>
        </div>
      </div>
    </div>
  );
}
