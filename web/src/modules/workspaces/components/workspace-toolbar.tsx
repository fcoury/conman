import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Check, File, Loader2, Save } from 'lucide-react';

interface WorkspaceToolbarProps {
  filePath: string | null;
  isDirty: boolean;
  saveStatus: 'idle' | 'saving' | 'saved' | 'error';
  onSave: () => void;
}

export default function WorkspaceToolbar({
  filePath,
  isDirty,
  saveStatus,
  onSave,
}: WorkspaceToolbarProps) {
  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b bg-muted/30 px-3">
      {filePath ? (
        <>
          <File className="size-3.5 text-muted-foreground" />
          <span className="text-sm text-muted-foreground">
            {filePath}
            {isDirty && <span className="ml-1 text-amber-500">*</span>}
          </span>
        </>
      ) : (
        <span className="text-sm text-muted-foreground">No file open</span>
      )}

      <div className="ml-auto flex items-center gap-2">
        {saveStatus === 'saving' && (
          <span className="flex items-center gap-1 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            Saving...
          </span>
        )}
        {saveStatus === 'saved' && (
          <span className="flex items-center gap-1 text-xs text-green-500">
            <Check className="size-3" />
            Saved
          </span>
        )}
        {saveStatus === 'error' && (
          <span className="text-xs text-destructive">Save failed</span>
        )}

        {filePath && (
          <>
            <Separator orientation="vertical" className="h-4" />
            <Button
              variant="ghost"
              size="sm"
              className="h-7 gap-1 px-2 text-xs"
              disabled={!isDirty || saveStatus === 'saving'}
              onClick={onSave}
            >
              <Save className="size-3" />
              Save
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
