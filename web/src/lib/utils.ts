import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function formatDate(value?: string | null): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return date.toLocaleString();
}

export function prettifyJson(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

export function fileExtension(path: string): string {
  const chunks = path.split(".");
  if (chunks.length < 2) return "";
  return chunks.at(-1)?.toLowerCase() ?? "";
}

export function isProbablyTextFile(path: string): boolean {
  const extension = fileExtension(path);
  const textExtensions = new Set([
    "yml",
    "yaml",
    "json",
    "js",
    "jsx",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "css",
    "html",
    "md",
    "txt",
    "toml",
    "env",
    "ini",
    "graphql",
    "sh",
    "xml",
  ]);
  return textExtensions.has(extension);
}
