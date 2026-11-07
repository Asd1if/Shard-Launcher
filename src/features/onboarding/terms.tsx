import { useTranslation } from "react-i18next";

/** Версия условий: при существенных изменениях увеличиваем, и лаунчер попросит принять их заново. */
export const TERMS_VERSION = 1;

/** Условия использования и что лаунчер делает с данными — коротко, по пунктам. */
export function TermsText() {
  const { t } = useTranslation();
  const sections = t("terms.sections", { returnObjects: true }) as { title: string; items: string[] }[];
  return (
    <div className="flex flex-col gap-4 text-[13px] leading-relaxed text-ink">
      {sections.map((s) => (
        <section key={s.title} className="flex flex-col gap-1.5">
          <h3 className="text-foreground">{s.title}</h3>
          <ul className="flex list-disc flex-col gap-1 pl-5 text-ink-muted marker:text-surface-3">
            {s.items.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
