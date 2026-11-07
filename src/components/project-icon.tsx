import { useState } from "react";
import { Package } from "lucide-react";
import { cn } from "@/lib/utils";

/** Иконка проекта; без картинки — заглушка в цветах темы. */
export function ProjectIcon({ url, className }: { url: string | null | undefined; className?: string }) {
  const [broken, setBroken] = useState(false);
  if (!url || broken)
    return (
      <span className={cn("grid shrink-0 place-items-center rounded-md bg-surface-3 text-ink-muted", className)}>
        <Package className="size-1/2" />
      </span>
    );
  return (
    <img
      src={url}
      alt=""
      loading="lazy"
      onError={() => setBroken(true)}
      className={cn("shrink-0 rounded-md bg-surface-3 object-cover", className)}
    />
  );
}
