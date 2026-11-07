import i18n from "@/i18n";

/**
 * Бэкенд отдаёт известные ошибки кодом (`auth:<код>`, `java_too_old:<было>:<нужно>` …) —
 * переводим их на язык интерфейса, остальные показываем как есть.
 */
export function errorText(error: unknown) {
  const text = error instanceof Error ? error.message : String(error);
  const auth = /auth:([a-z_]+)/.exec(text);
  if (auth && i18n.exists(`auth.errors.${auth[1]}`)) return i18n.t(`auth.errors.${auth[1]}`);
  const javaOld = /java_too_old:(\d+):(\d+)/.exec(text);
  if (javaOld) return i18n.t("errors.javaTooOld", { found: javaOld[1], required: javaOld[2] });
  const preLaunch = /pre_launch_failed:(\S+)/.exec(text);
  if (preLaunch) return i18n.t("errors.preLaunchFailed", { code: preLaunch[1] });
  if (text.includes("icon_not_image")) return i18n.t("errors.iconNotImage");
  if (text.includes("skin_not_png")) return i18n.t("errors.skinNotPng");
  const badSize = text.match(/skin_bad_size:(\d+x\d+)/);
  if (badSize) return i18n.t("errors.skinBadSize", { size: badSize[1].replace("x", "×") });
  if (text.includes("skin_player_not_found")) return i18n.t("errors.skinPlayerNotFound");
  if (text.includes("skin_player_default")) return i18n.t("errors.skinPlayerDefault");
  if (text.includes("skin_needs_microsoft")) return i18n.t("errors.skinNeedsMicrosoft");
  if (text.includes("skin_preview_unavailable")) return i18n.t("errors.skinPreviewUnavailable");
  const apiFailed = text.match(/skin_api_failed: (.*)/);
  if (apiFailed) return i18n.t("errors.skinApiFailed", { message: apiFailed[1] });
  if (text.includes("pack_unknown_format")) return i18n.t("errors.packUnknownFormat");
  if (text.includes("pack_bad_manifest")) return i18n.t("errors.packBadManifest");
  if (text.includes("pack_manual_download")) return i18n.t("errors.packManualDownload");
  if (text.includes("pack_not_found")) return i18n.t("errors.packNotFound");
  const packManual = /server_pack_manual:(\S*)/.exec(text);
  if (packManual) return i18n.t("errors.serverPackManual", { url: packManual[1] });
  const portBusy = /server_port_busy:(\d+)/.exec(text);
  if (portBusy) return i18n.t("errors.serverPortBusy", { port: portBusy[1] });
  const serverInstall = /server_install_failed:?([\s\S]*)/.exec(text);
  if (serverInstall) return i18n.t("errors.serverInstallFailed", { detail: serverInstall[1].trim().split("\n").pop() ?? "" });
  const tunnelApi = /tunnel_api:(.*)/.exec(text);
  if (tunnelApi) return i18n.t("errors.tunnelApi", { reason: tunnelApi[1] });
  const codes: Record<string, string> = {
    tunnel_link_rejected: "tunnelLinkRejected",
    tunnel_link_expired: "tunnelLinkExpired",
    tunnel_not_linked: "tunnelNotLinked",
    tunnel_agent_missing: "tunnelAgentMissing",
    tunnel_pending: "tunnelPending",
    world_not_found: "worldNotFound",
    file_not_found: "fileNotFound",
    server_invalid: "serverInvalid",
    pack_no_icon: "packNoIcon",
    pack_not_linked: "packNotLinked",
    data_dir_portable: "dataDirPortable",
    data_dir_game_running: "dataDirGameRunning",
    data_dir_invalid: "dataDirInvalid",
    data_dir_nested: "dataDirNested",
    data_dir_not_empty: "dataDirNotEmpty",
    background_too_large: "backgroundTooLarge",
    figura_not_avatar: "figuraNotAvatar",
    server_eula: "serverEula",
    server_pack_missing: "serverPackMissing",
    logs_empty: "logsEmpty",
    cpm_not_model: "cpmNotModel",
    server_not_running: "serverNotRunning",
    server_running: "serverRunning",
    server_missing: "serverMissing",
    server_store_vanilla: "serverStoreVanilla",
    bad_nick: "badNick",
    bad_path: "badPath",
    instance_running: "instanceRunning",
    options_missing: "optionsMissing",
    update_game_running: "updateGameRunning",
    update_none: "updateNone",
    update_bad_asset: "updateBadAsset",
    file_too_large: "fileTooLarge",
    file_binary: "fileBinary",
    player_not_found: "playerNotFound",
    server_no_source: "serverNoSource",
    server_core_unavailable: "serverCoreUnavailable",
    server_unavailable: "serverUnavailable",
    server_no_loader_version: "serverNoLoaderVersion",
    server_port_invalid: "serverPortInvalid",
    server_property_invalid: "serverPropertyInvalid",
  };
  for (const [code, key] of Object.entries(codes)) if (text.includes(code)) return i18n.t(`errors.${key}`);
  if (text.includes("curseforge_no_key")) return i18n.t("errors.curseforgeNoKey");
  if (text.includes("curseforge_bad_key")) return i18n.t("errors.curseforgeBadKey");
  if (text.includes("curseforge_manual_update")) return i18n.t("errors.curseforgeManualUpdate");
  if (text.includes("no_compatible_version")) return i18n.t("errors.noCompatibleVersion");
  if (text.includes("content_needs_loader")) return i18n.t("errors.contentNeedsLoader");
  if (text.includes("shaders_need_loader")) return i18n.t("errors.shadersNeedLoader");
  if (text.includes("loader_unavailable")) return i18n.t("errors.loaderUnavailable");
  // Хвост вывода процессора установщика остаётся в консоли, в уведомление — первая строка
  const loaderFailed = /loader_install_failed: ?(.*)/.exec(text);
  if (loaderFailed) return i18n.t("errors.loaderInstallFailed", { detail: loaderFailed[1] });
  if (text.includes("java_not_runnable") || text.includes("java_probe_timeout")) return i18n.t("errors.javaNotRunnable");
  return text;
}

export function isAuthError(error: unknown, code: string) {
  return String(error instanceof Error ? error.message : error).includes(`auth:${code}`);
}
