import { createContext, useCallback, useContext, useState } from "react";
import { X, CheckCircle, AlertTriangle, Info } from "lucide-react";
import { cn } from "@/lib/utils";

type ToastType = "success" | "error" | "info";

interface Toast {
  id: number;
  type: ToastType;
  message: string;
}

interface ToastContextValue {
  toast: (type: ToastType, message: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

let toastId = 0;

export function ToastProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((type: ToastType, message: string) => {
    const id = ++toastId;
    setToasts((prev) => [...prev, { id, type, message }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 4000);
  }, []);

  const removeToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toast: addToast }}>
      {children}
      <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2">
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} onDismiss={() => removeToast(t.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

const iconMap: Record<ToastType, typeof CheckCircle> = {
  success: CheckCircle,
  error: AlertTriangle,
  info: Info,
};

const styleMap: Record<ToastType, string> = {
  success: "border-success/30 text-success-foreground",
  error: "border-destructive/30 text-destructive",
  info: "border-primary/30 text-primary",
};

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: () => void }): React.ReactElement {
  const Icon = iconMap[toast.type];
  return (
    <div
      className={cn(
        "flex items-center gap-2 rounded-lg border bg-popover px-4 py-3 shadow-lg shadow-black/20 animate-fade-in-up min-w-[280px] max-w-[400px]",
        styleMap[toast.type],
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className="text-sm text-popover-foreground flex-1">{toast.message}</span>
      <button type="button" onClick={onDismiss} className="text-muted-foreground hover:text-foreground cursor-pointer">
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used within ToastProvider");
  return ctx;
}
