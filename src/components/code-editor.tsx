import { useMemo, useRef, type KeyboardEvent, type ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * Простой редактор конфигов с подсветкой: прозрачный textarea поверх <pre> с раскрашенным
 * текстом. Понимает properties/ini/cfg, YAML, JSON и TOML — комментарии, ключи, секции,
 * строки, числа и true/false. Большие файлы показываются без подсветки, чтобы не тормозить.
 */

type Token = "comment" | "key" | "section" | "string" | "number" | "bool" | "punct";

const HIGHLIGHT_LIMIT = 300_000;

const tokenClass: Record<Token, string> = {
  comment: "text-[var(--code-comment)] italic",
  key: "text-[var(--code-key)]",
  section: "text-[var(--code-section)] font-semibold",
  string: "text-[var(--code-string)]",
  number: "text-[var(--code-number)]",
  bool: "text-[var(--code-bool)]",
  punct: "text-[var(--code-punct)]",
};

type Lang = "props" | "yaml" | "json" | "toml";

export function langOf(path: string): Lang | null {
  const ext = path.toLowerCase().split(".").pop() ?? "";
  if (["properties", "cfg", "conf", "ini", "env", "txt", "lang"].includes(ext)) return "props";
  if (["yml", "yaml"].includes(ext)) return "yaml";
  if (["json", "json5", "mcmeta", "jsonc"].includes(ext)) return "json";
  if (ext === "toml") return "toml";
  return null;
}

const SCALAR = /^(true|false|null|yes|no|on|off|~)$/i;
const NUMBER = /^-?\d+(\.\d+)?([eE][+-]?\d+)?[dDfFlLbBsS]?$/;

/** Значение после = или : — строка, число, булево или просто текст. */
function value(text: string, out: ReactNode[], key: number) {
  const trimmed = text.trim();
  const lead = text.slice(0, text.length - text.trimStart().length);
  const tail = text.slice(lead.length + trimmed.length);
  let kind: Token | null = null;
  if (/^(["']).*\1$/.test(trimmed)) kind = "string";
  else if (NUMBER.test(trimmed)) kind = "number";
  else if (SCALAR.test(trimmed)) kind = "bool";
  out.push(lead);
  out.push(kind ? <span key={key} className={tokenClass[kind]}>{trimmed}</span> : trimmed);
  out.push(tail);
}

/** Разбор строки «по словам» для JSON и значений внутри массивов. */
function inline(text: string, out: ReactNode[], base: number) {
  const re = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')(\s*:)?|(-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b)|\b(true|false|null)\b|([{}[\],])|(\/\/.*$)/g;
  let last = 0;
  let i = base;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    out.push(text.slice(last, m.index));
    if (m[1]) {
      out.push(<span key={i++} className={tokenClass[m[2] ? "key" : "string"]}>{m[1]}</span>);
      if (m[2]) out.push(<span key={i++} className={tokenClass.punct}>{m[2]}</span>);
    } else if (m[3]) out.push(<span key={i++} className={tokenClass.number}>{m[3]}</span>);
    else if (m[4]) out.push(<span key={i++} className={tokenClass.bool}>{m[4]}</span>);
    else if (m[5]) out.push(<span key={i++} className={tokenClass.punct}>{m[5]}</span>);
    else if (m[6]) out.push(<span key={i++} className={tokenClass.comment}>{m[6]}</span>);
    last = m.index + m[0].length;
  }
  out.push(text.slice(last));
}

function line(text: string, lang: Lang, n: number): ReactNode[] {
  const out: ReactNode[] = [];
  const k = n * 1000;
  const trimmed = text.trimStart();
  const indent = text.slice(0, text.length - trimmed.length);

  if (lang === "json") {
    inline(text, out, k);
    return out;
  }
  // Комментарии на всю строку
  if (/^(#|;|!|\/\/)/.test(trimmed)) {
    return [indent, <span key={k} className={tokenClass.comment}>{trimmed}</span>];
  }
  // [секция] в ini/toml/старых cfg
  if ((lang === "props" || lang === "toml") && /^\[.*\]\s*$/.test(trimmed)) {
    return [indent, <span key={k} className={tokenClass.section}>{trimmed}</span>];
  }
  // Хвостовой комментарий « # ...» (не внутри кавычек — упрощённо: после пробела)
  let body = trimmed;
  let comment = "";
  const hash = lang === "props" ? -1 : body.search(/\s#/);
  if (hash > 0 && !/["']/.test(body.slice(0, hash))) {
    comment = body.slice(hash);
    body = body.slice(0, hash);
  }
  out.push(indent);
  const sep = lang === "yaml" ? body.search(/:(\s|$)/) : body.search(/[=:]/);
  // В YAML строка списка «- значение»
  const dash = lang === "yaml" && /^-\s/.test(body);
  if (dash) {
    out.push(<span key={k} className={tokenClass.punct}>-</span>);
    body = body.slice(1);
  }
  const sepAt = dash ? body.search(/:(\s|$)/) : sep;
  if (sepAt > 0) {
    out.push(<span key={k + 1} className={tokenClass.key}>{body.slice(0, sepAt)}</span>);
    out.push(<span key={k + 2} className={tokenClass.punct}>{body[sepAt]}</span>);
    const rest = body.slice(sepAt + 1);
    if (/^\s*[[{]/.test(rest)) inline(rest, out, k + 3);
    else value(rest, out, k + 3);
  } else if (body) {
    value(body, out, k + 3);
  }
  if (comment) out.push(<span key={k + 999} className={tokenClass.comment}>{comment}</span>);
  return out;
}

export function CodeEditor({
  value: text,
  path,
  onChange,
  onKeyDown,
  className,
}: {
  value: string;
  path: string;
  onChange: (text: string) => void;
  onKeyDown?: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  className?: string;
}) {
  const lang = langOf(path);
  const highlight = lang !== null && text.length <= HIGHLIGHT_LIMIT;
  const pre = useRef<HTMLPreElement>(null);
  const lines = useMemo(() => (highlight ? text.split("\n").map((l, i) => line(l, lang!, i)) : null), [text, lang, highlight]);
  const base = "m-0 h-full w-full overflow-auto whitespace-pre p-3 font-mono text-[12.5px] leading-relaxed [tab-size:4]";

  return (
    <div className={cn("relative overflow-hidden rounded-lg border border-surface-3 bg-surface-1 focus-within:ring-2 focus-within:ring-ring", className)}>
      {lines && (
        <pre ref={pre} aria-hidden className={cn(base, "pointer-events-none absolute inset-0 text-foreground")}>
          {lines.map((l, i) => (
            <div key={i}>{l.length ? l : "​"}</div>
          ))}
          {/* Запас под последнюю строку и горизонтальную полосу прокрутки */}
          <div>{"​"}</div>
        </pre>
      )}
      <textarea
        value={text}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          // Tab вставляет отступ, а не уводит фокус
          if (e.key === "Tab" && !e.shiftKey && !e.ctrlKey) {
            e.preventDefault();
            const el = e.currentTarget;
            const { selectionStart: s, selectionEnd: end } = el;
            const indent = lang === "yaml" ? "  " : "\t";
            onChange(text.slice(0, s) + indent + text.slice(end));
            requestAnimationFrame(() => el.setSelectionRange(s + indent.length, s + indent.length));
            return;
          }
          onKeyDown?.(e);
        }}
        onScroll={(e) => {
          if (pre.current) {
            pre.current.scrollTop = e.currentTarget.scrollTop;
            pre.current.scrollLeft = e.currentTarget.scrollLeft;
          }
        }}
        spellCheck={false}
        wrap="off"
        aria-label={path}
        className={cn(
          base,
          "relative resize-none bg-transparent outline-none",
          lines ? "text-transparent caret-foreground selection:bg-brand/30 selection:text-transparent" : "text-foreground",
        )}
      />
    </div>
  );
}
