/** Размер файла по-человечески: «1,2 GB». */
export function formatSize(bytes: number, lang: string) {
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${new Intl.NumberFormat(lang, { maximumFractionDigits: i ? 1 : 0 }).format(n)} ${units[i]}`;
}
