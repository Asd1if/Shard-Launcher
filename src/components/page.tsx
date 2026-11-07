import { cn } from "@/lib/utils";

export function Page({
  title,
  actions,
  children,
  className,
}: {
  title?: string;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("mx-auto flex w-full max-w-[1120px] flex-col gap-6 px-8 py-7", className)}>
      {title && (
        <div className="flex items-end justify-between gap-4">
          <h1 className="font-display text-2xl font-medium tracking-tight">{title}</h1>
          {actions && <div className="flex items-center gap-2">{actions}</div>}
        </div>
      )}
      {children}
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  text,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  text: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-start gap-3 rounded-lg border border-dashed border-surface-3 px-8 py-10">
      <div className="text-brand">{icon}</div>
      <h2 className="font-display text-base font-medium">{title}</h2>
      <p className="max-w-[52ch] text-ink-muted">{text}</p>
      {action && <div className="pt-2">{action}</div>}
    </div>
  );
}
