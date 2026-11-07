import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ru from "./ru.json";
import en from "./en.json";

export const languages = [
  { code: "ru", label: "Русский" },
  { code: "en", label: "English" },
] as const;

export type Language = (typeof languages)[number]["code"];

const STORAGE_KEY = "shard.language";

function initialLanguage(): Language {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "ru" || saved === "en") return saved;
  } catch {
    // хранилище недоступно — берём язык системы
  }
  return navigator.language.startsWith("ru") ? "ru" : "en";
}

export function setLanguage(lang: Language) {
  void i18n.changeLanguage(lang);
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // не критично: язык просто не запомнится
  }
}

void i18n.use(initReactI18next).init({
  resources: { ru: { translation: ru }, en: { translation: en } },
  lng: initialLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});
document.documentElement.lang = i18n.language;

export default i18n;
