import { useEffect, useRef, useState } from 'react';
import { File, Folder } from 'lucide-react';

interface FileTreeInputProps {
  type: 'file' | 'folder';
  depth: number;
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

export default function FileTreeInput({
  type,
  depth,
  onConfirm,
  onCancel,
}: FileTreeInputProps) {
  const [value, setValue] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-focus on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      const trimmed = value.trim();
      if (trimmed && !trimmed.includes('/')) {
        onConfirm(trimmed);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  }

  const Icon = type === 'folder' ? Folder : File;

  return (
    <div
      className="flex items-center gap-1.5 px-2 py-1"
      style={{ paddingLeft: `${depth * 16 + 8}px` }}
    >
      {/* Spacer matching the chevron area */}
      <span className="w-3.5 shrink-0" />
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <input
        ref={inputRef}
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={onCancel}
        placeholder={type === 'folder' ? 'Folder name' : 'File name'}
        className="h-5 flex-1 rounded-sm border-0 bg-transparent px-1 text-sm outline-none ring-1 ring-primary/50 placeholder:text-muted-foreground/60"
      />
    </div>
  );
}
