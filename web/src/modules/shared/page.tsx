export function Page({ title, description, children }: { title: string; description?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="space-y-6 animate-fade-in-up">
      <div>
        <h1 className="text-xl font-semibold font-heading tracking-tight">{title}</h1>
        {description ? <p className="text-sm text-muted-foreground mt-1">{description}</p> : null}
      </div>
      {children}
    </div>
  );
}
