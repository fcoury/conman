import { Card, CardDescription, CardTitle } from "@/components/ui/card";

export function Page({ title, description, children }: { title: string; description?: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="space-y-4">
      <Card className="p-5">
        <CardTitle>{title}</CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </Card>
      {children}
    </div>
  );
}
