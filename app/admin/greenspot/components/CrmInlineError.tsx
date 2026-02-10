"use client";

import type { ReactNode } from "react";

type CrmInlineErrorProps = {
  title?: string;
  message: string;
  action?: ReactNode;
};

export default function CrmInlineError({
  title = "Something went wrong",
  message,
  action,
}: CrmInlineErrorProps) {
  return (
    <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm">
      <div className="space-y-2">
        <p className="text-sm font-semibold text-red-800">{title}</p>
        <p className="text-sm text-red-600">{message}</p>
        {action ? <div className="pt-2">{action}</div> : null}
      </div>
    </div>
  );
}
