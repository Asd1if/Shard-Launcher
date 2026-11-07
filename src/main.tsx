import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter, Route, Routes } from "react-router";
import "./index.css";
import "./i18n";
import { AppLayout } from "@/app/layout";
import { HomePage } from "@/routes/home";

// Сразу грузится только главная; остальные страницы — отдельными файлами
const pages = {
  instances: () => import("@/routes/instances"),
  modpacks: () => import("@/routes/modpacks"),
  servers: () => import("@/routes/servers"),
  settings: () => import("@/routes/settings"),
  skins: () => import("@/routes/skins"),
};

function page<K extends keyof typeof pages>(key: K, name: string) {
  const Lazy = React.lazy(() => pages[key]().then((m) => ({ default: (m as unknown as Record<string, React.ComponentType>)[name] })));
  return (
    <React.Suspense fallback={null}>
      <Lazy />
    </React.Suspense>
  );
}

// Остальные страницы догружаются в фоне, когда главная уже на экране: переходы потом мгновенные
const preload = () => Object.values(pages).forEach((load) => void load());
if ("requestIdleCallback" in window) window.requestIdleCallback(preload, { timeout: 2500 });
else setTimeout(preload, 1500);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<HomePage />} />
          <Route path="instances" element={page("instances", "InstancesPage")} />
          <Route path="instances/:id" element={page("instances", "InstancePage")} />
          <Route path="modpacks" element={page("modpacks", "ModpacksPage")} />
          <Route path="servers" element={page("servers", "ServersPage")} />
          <Route path="servers/new" element={page("servers", "NewServerPage")} />
          <Route path="servers/:id" element={page("servers", "ServerPage")} />
          <Route path="skins" element={page("skins", "SkinsPage")} />
          <Route path="settings" element={page("settings", "SettingsPage")} />
        </Route>
      </Routes>
    </HashRouter>
  </React.StrictMode>,
);
