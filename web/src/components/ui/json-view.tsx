import { prettifyJson } from "@/lib/utils";

export function JsonView({ value }: { value: unknown }): React.ReactElement {
  return (
    <pre className="bg-background border border-border max-h-120 overflow-auto rounded-md p-3 text-xs text-foreground/80 whitespace-pre-wrap break-all">
      {prettifyJson(value)}
    </pre>
  );
}
