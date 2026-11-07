// Тема до первого кадра (отдельным файлом, чтобы CSP не разрешал встроенные скрипты)
try {
  var t = localStorage.getItem("shard.theme");
  var dark = window.matchMedia("(prefers-color-scheme: dark)").matches ? "graphite" : "light";
  if (t === "system") t = dark;
  if (t === "auto") {
    var b = JSON.parse(localStorage.getItem("shard.backdrop") || "null");
    t = b && b.kind === "custom" && b.custom && typeof b.luma === "number" ? (b.luma > 0.52 ? "light" : "graphite") : dark;
  }
  if (["graphite", "light", "amethyst", "emerald", "amber", "ocean", "rose"].indexOf(t) >= 0) document.documentElement.dataset.theme = t;
} catch (e) {}
