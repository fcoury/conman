import { Card, CardTitle } from "@/components/ui/card";
import { JsonView } from "@/components/ui/json-view";

export function RawDataPanel({
  title = "Advanced data",
  value,
}: {
  title?: string;
  value: unknown;
}): React.ReactElement {
  return (
    <details className="group">
      <summary className="cursor-pointer list-none rounded-md border border-border bg-muted/40 px-3 py-2 text-sm font-medium text-foreground transition-colors hover:bg-muted">
        {title}
      </summary>
      <Card className="mt-2">
        <CardTitle className="text-sm">{title}</CardTitle>
        <div className="mt-3">
          <JsonView value={value} />
        </div>
      </Card>
    </details>
  );
}
