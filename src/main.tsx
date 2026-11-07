import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter, Route, Routes } from "react-router";
import "./index.css";
import "./i18n";
import { AppLayout } from "@/app/layout";
import { HomePage } from "@/routes/home";
import { InstancePage, InstancesPage } from "@/routes/instances";
import { ModpacksPage } from "@/routes/modpacks";
import { NewServerPage, ServerPage, ServersPage } from "@/routes/servers";
import { SettingsPage } from "@/routes/settings";

// three.js для 3D-скина нужен только на этой странице — грузим её отдельно
const SkinsPage = React.lazy(() => import("@/routes/skins").then((m) => ({ default: m.SkinsPage })));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <Routes>
        <Route element={<AppLayout />}>
          <Route index element={<HomePage />} />
          <Route path="instances" element={<InstancesPage />} />
          <Route path="instances/:id" element={<InstancePage />} />
          <Route path="modpacks" element={<ModpacksPage />} />
          <Route path="servers" element={<ServersPage />} />
          <Route path="servers/new" element={<NewServerPage />} />
          <Route path="servers/:id" element={<ServerPage />} />
          <Route
            path="skins"
            element={
              <React.Suspense fallback={null}>
                <SkinsPage />
              </React.Suspense>
            }
          />
          <Route path="settings" element={<SettingsPage />} />
        </Route>
      </Routes>
    </HashRouter>
  </React.StrictMode>,
);
