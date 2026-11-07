import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";

/**
 * Плащ с текстуры 64×32 (или HD кратно больше): лицевая сторона плаща и крыло элитр рядом.
 * Плащ — участок 1,1 размером 10×16, внешняя сторона крыла — 34,2 размером 12×20.
 */
export function CapeThumb({ src, className }: { src: string | null; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const c = canvas.current!;
    const ctx = c.getContext("2d")!;
    ctx.clearRect(0, 0, c.width, c.height);
    if (!src) return;
    const img = new Image();
    img.onload = () => {
      const k = img.width / 64;
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, c.width, c.height);
      ctx.drawImage(img, 1 * k, 1 * k, 10 * k, 16 * k, 0, 2, 10, 16);
      ctx.drawImage(img, 34 * k, 2 * k, 12 * k, 20 * k, 13, 0, 12, 20);
    };
    img.src = src;
  }, [src]);

  return <canvas ref={canvas} width={25} height={20} className={cn("[image-rendering:pixelated]", className)} />;
}
