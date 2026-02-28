export function parentPath(path: string): string {
  const trimmed = path.trim().replace(/^\/+|\/+$/g, "");
  if (!trimmed) {
    return "";
  }
  const parts = trimmed.split("/");
  parts.pop();
  return parts.join("/");
}
