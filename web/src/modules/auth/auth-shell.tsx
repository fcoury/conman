import type { ReactNode } from 'react';

import { Card } from '@/components/ui/card';

interface AuthShellProps {
  title: string;
  subtitle: string;
  children: ReactNode;
}

export default function AuthShell({ title, subtitle, children }: AuthShellProps) {
  return (
    <div className="flex min-h-screen items-center justify-center bg-muted/40 px-4 py-8">
      <Card className="w-full max-w-md border border-border bg-background shadow-sm">
        <div className="space-y-1 border-b border-border px-6 py-5">
          <h1 className="text-2xl font-semibold">{title}</h1>
          <p className="text-sm text-muted-foreground">{subtitle}</p>
        </div>
        <div className="px-6 py-5">{children}</div>
      </Card>
    </div>
  );
}
