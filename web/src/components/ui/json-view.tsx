import { prettifyJson } from "@/lib/utils";

export function JsonView({ value }: { value: unknown }): React.ReactElement {
  return (
    <pre className="bg-muted max-h-120 overflow-auto rounded-md p-3 text-xs whitespace-pre-wrap break-all">
      {prettifyJson(value)}
    </pre>
  );
}
