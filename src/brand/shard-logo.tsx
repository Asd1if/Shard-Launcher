import art from "./shard-pixels.json";

const colors = art.colors as Record<string, string>;

/** Пиксельный кристалл Shard без плитки — для titlebar и шапки. */
export function ShardLogo({ className }: { className?: string }) {
  return (
    <svg
      viewBox="3 2 10 12"
      className={className}
      shapeRendering="crispEdges"
      aria-hidden
    >
      {art.rows.flatMap((row, y) =>
        [...row].map((ch, x) =>
          colors[ch] ? (
            <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} className={`shard-${ch}`} />
          ) : null,
        ),
      )}
    </svg>
  );
}
