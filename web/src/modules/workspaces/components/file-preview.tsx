import { useEffect, useMemo, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Download, File, Image as ImageIcon, Type } from 'lucide-react';

interface FilePreviewProps {
  filePath: string;
  content: string; // base64
  size: number;
  type: 'image' | 'font' | 'binary';
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function mimeFromPath(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  const mimeMap: Record<string, string> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    svg: 'image/svg+xml',
    webp: 'image/webp',
    ico: 'image/x-icon',
    ttf: 'font/ttf',
    woff: 'font/woff',
    woff2: 'font/woff2',
    otf: 'font/otf',
  };
  return mimeMap[ext] ?? 'application/octet-stream';
}

function ImagePreview({ filePath, content, size }: Omit<FilePreviewProps, 'type'>) {
  const dataUri = `data:${mimeFromPath(filePath)};base64,${content}`;
  const fileName = filePath.split('/').pop() ?? filePath;

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-6">
      {/* Checkerboard background for transparency */}
      <div
        className="flex max-h-[60vh] max-w-full items-center justify-center overflow-hidden rounded-lg border"
        style={{
          backgroundImage:
            'linear-gradient(45deg, #808080 25%, transparent 25%), linear-gradient(-45deg, #808080 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #808080 75%), linear-gradient(-45deg, transparent 75%, #808080 75%)',
          backgroundSize: '16px 16px',
          backgroundPosition: '0 0, 0 8px, 8px -8px, -8px 0px',
        }}
      >
        <img
          src={dataUri}
          alt={fileName}
          className="max-h-[60vh] max-w-full object-contain"
        />
      </div>
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <ImageIcon className="size-4" />
        <span>{fileName}</span>
        <span>&middot;</span>
        <span>{formatSize(size)}</span>
      </div>
    </div>
  );
}

function FontPreview({ filePath, content, size }: Omit<FilePreviewProps, 'type'>) {
  const fontName = useMemo(() => `preview-${filePath.replace(/[^a-zA-Z0-9]/g, '-')}`, [filePath]);
  const styleRef = useRef<HTMLStyleElement | null>(null);

  useEffect(() => {
    const style = document.createElement('style');
    style.textContent = `
      @font-face {
        font-family: '${fontName}';
        src: url('data:${mimeFromPath(filePath)};base64,${content}') format('${filePath.endsWith('.woff2') ? 'woff2' : filePath.endsWith('.woff') ? 'woff' : 'truetype'}');
      }
    `;
    document.head.appendChild(style);
    styleRef.current = style;
    return () => {
      if (styleRef.current) {
        document.head.removeChild(styleRef.current);
      }
    };
  }, [fontName, filePath, content]);

  const fileName = filePath.split('/').pop() ?? filePath;
  const specimen = 'The quick brown fox jumps over the lazy dog';

  return (
    <div className="flex h-full flex-col gap-6 overflow-auto p-6">
      <div className="flex items-center gap-2 text-sm text-muted-foreground">
        <Type className="size-4" />
        <span>{fileName}</span>
        <span>&middot;</span>
        <span>{formatSize(size)}</span>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Specimen</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4" style={{ fontFamily: `'${fontName}'` }}>
          <p className="text-lg">ABCDEFGHIJKLMNOPQRSTUVWXYZ</p>
          <p className="text-lg">abcdefghijklmnopqrstuvwxyz</p>
          <p className="text-lg">0123456789 !@#$%^&*()</p>
          {[16, 24, 32, 48].map((sz) => (
            <p key={sz} style={{ fontSize: `${sz}px`, lineHeight: 1.3 }}>
              {specimen}
            </p>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function BinaryPreview({ filePath, content, size }: Omit<FilePreviewProps, 'type'>) {
  const fileName = filePath.split('/').pop() ?? filePath;

  function handleDownload() {
    const binary = atob(content);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    const blob = new Blob([bytes], { type: mimeFromPath(filePath) });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = fileName;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-6">
      <File className="size-16 text-muted-foreground" />
      <div className="text-center">
        <p className="text-sm font-medium">{fileName}</p>
        <p className="text-xs text-muted-foreground">{formatSize(size)}</p>
      </div>
      <Button variant="outline" size="sm" onClick={handleDownload}>
        <Download className="mr-2 size-4" />
        Download
      </Button>
    </div>
  );
}

export default function FilePreview({ filePath, content, size, type }: FilePreviewProps) {
  switch (type) {
    case 'image':
      return <ImagePreview filePath={filePath} content={content} size={size} />;
    case 'font':
      return <FontPreview filePath={filePath} content={content} size={size} />;
    case 'binary':
      return <BinaryPreview filePath={filePath} content={content} size={size} />;
  }
}
