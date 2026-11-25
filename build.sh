#!/usr/bin/env bash
# Сборка Shard для Linux: AppImage, .deb и .rpm в папку release/.
#   ./build.sh            — все форматы
#   ./build.sh appimage   — только нужные (через запятую: deb,rpm)
#   ./build.sh --deps     — только установить системные зависимости
set -euo pipefail
cd "$(dirname "$0")"

# Пакеты для сборки Tauri (WebKitGTK 4.1) по семействам дистрибутивов
install_deps() {
  local id like
  id=$(. /etc/os-release && echo "${ID:-}")
  like=$(. /etc/os-release && echo "${ID_LIKE:-}")
  case " $id $like " in
    *" arch "*)
      sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl librsvg xdotool ;;
    *" fedora "* | *" rhel "*)
      sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file librsvg2-devel libxdo-devel dbus-devel
      sudo dnf group install -y c-development 2>/dev/null || sudo dnf groupinstall -y "C Development Tools and Libraries" ;;
    *" opensuse"* | *" suse "*)
      sudo zypper in -y webkit2gtk3-devel libopenssl-devel curl wget file librsvg-devel dbus-1-devel
      sudo zypper in -y -t pattern devel_basis ;;
    *" debian "* | *" ubuntu "*)
      sudo apt-get update
      sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libdbus-1-dev patchelf ;;
    *)
      echo "Неизвестный дистрибутив ($id). Поставьте WebKitGTK 4.1, librsvg, libdbus и средства сборки C вручную:"
      echo "https://tauri.app/start/prerequisites/#linux"
      return 1 ;;
  esac
}

if [[ "${1:-}" == "--deps" ]]; then
  install_deps
  exit
fi

need() { command -v "$1" >/dev/null 2>&1 || { echo "Не найден $1. $2"; exit 1; }; }
need npm "Установите Node.js 20+ (https://nodejs.org или пакетом nodejs)."
need cargo "Установите Rust: https://rustup.rs (или пакетом rustup)."

if ! pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  echo "Нет WebKitGTK 4.1 для сборки. Ставлю системные зависимости (понадобится пароль sudo)…"
  install_deps
fi

# Client ID Microsoft вшивается при сборке. Без него Shard работает, но без входа Microsoft.
if [[ ! -s src-tauri/msa-client-id.txt && -z "${SHARD_MSA_CLIENT_ID:-}" ]]; then
  echo "Внимание: нет src-tauri/msa-client-id.txt — вход через Microsoft будет недоступен."
fi
# Публичная сборка — без ключа CurseForge: пользователь вводит свой в настройках
export SHARD_PUBLIC_BUILD="${SHARD_PUBLIC_BUILD:-1}"

[[ -d node_modules ]] || npm ci

bundles="${1:-deb,rpm,appimage}"
echo "Собираю Shard ($bundles). С оптимизацией это 5–15 минут."
# Без этого linuxdeploy падает на некоторых системах (Arch, Fedora) при сборке AppImage
export NO_STRIP="${NO_STRIP:-true}"
npm run tauri build -- --bundles "$bundles"

version=$(node -p "require('./package.json').version")
mkdir -p release
shopt -s nullglob
out=src-tauri/target/release/bundle
for f in "$out"/appimage/*.AppImage "$out"/deb/*.deb "$out"/rpm/*.rpm; do
  name=$(basename "$f")
  # «Shard Launcher_0.5.4_amd64.deb» → «Shard-Launcher_0.5.4_amd64.deb»
  cp -f "$f" "release/${name// /-}"
done
echo
echo "Готово ($version): $(pwd)/release"
ls -1 release
