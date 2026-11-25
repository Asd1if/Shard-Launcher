# Shard Launcher

Бесплатный лаунчер с открытым кодом для **Minecraft: Java Edition** на Tauri, Rust и React.
Работает на Windows и Linux (Arch, Fedora, Ubuntu/Debian, openSUSE и другие); macOS — в планах.

[English](README.md) · **Русский**

## Скачать

Всё лежит в [Releases](https://github.com/Asd1if/Shard-Launcher/releases):

| Система | Файл | Установка |
|---|---|---|
| Windows | `*-setup.exe` или `Shard-portable.zip` | запустить установщик / распаковать |
| Любой Linux | `Shard-Launcher_*_amd64.AppImage` | `chmod +x Shard-Launcher_*.AppImage` и запустить |
| Arch, Manjaro, EndeavourOS | [PKGBUILD](packaging/aur) или AppImage | `makepkg -si` в `packaging/aur` |
| Fedora, openSUSE, RHEL | `Shard-Launcher-*.x86_64.rpm` | `sudo dnf install ./Shard-Launcher-*.rpm` |
| Ubuntu, Debian, Mint | `Shard-Launcher_*_amd64.deb` | `sudo apt install ./Shard-Launcher_*.deb` |

Установщик Windows, портативная версия и AppImage обновляются сами. Пакеты deb, rpm и AUR
обновляет пакетный менеджер — лаунчер сообщит о новой версии и даст ссылку на релиз.

На Linux вход через Microsoft сохраняется в Secret Service (GNOME Keyring, KWallet, KeePassXC).
Без него вход работает до перезагрузки.

## Возможности

**Сборки**
- Vanilla, Fabric, Quilt, Forge и NeoForge; у каждой сборки своя папка, Java, память и аргументы
- Java скачивается сама, разбор вылетов с подсказками, как исправить
- Снимки сборки и откат в один клик, снимок делается сам перед обновлениями
- Бэкапы миров, скриншоты, статистика времени в игре
- Перенос настроек управления, звука и графики между сборками
- Оптимизация в один клик (Sodium/Embeddium, Lithium, FerriteCore и другие)
- Импорт из Prism Launcher, MultiMC, Modrinth App, CurseForge и `.minecraft`

**Моды и модпаки**
- Поиск на Modrinth и CurseForge с фильтрами по версии, загрузчику, категории и стороне
- Страница мода: описание, галерея, версии и зависимости
- Установка, обновление, включение и отключение; проверка недостающих зависимостей
- Установка, обновление и экспорт модпаков (`.mrpack`, `.zip` CurseForge)

**Серверы**
- Сервер из сборки или на Paper, Purpur, Fabric, Forge и других ядрах
- Консоль с цветами, поиском и подсказками команд
- Игроки, операторы, вайтлист и баны; файловый менеджер с подсветкой конфигов
- Авторестарт, перезапуск по расписанию, бэкапы, графики нагрузки
- Игра с друзьями по локальной сети или через туннель, сервер в списке игры в один клик

**И ещё**
- Вход через Microsoft и офлайн-профили, скины и плащи с 3D-превью
- Темы, включая «Системную» и «Авто», анимированные фоны или своя картинка
- Новости Minecraft на главной, статус в Discord, русский и английский интерфейс
- Встроенный MCP-сервер: ИИ-ассистенты могут смотреть сборки, ставить моды и запускать игру

## MCP-сервер

Shard умеет работать как [MCP](https://modelcontextprotocol.io)-сервер (Streamable HTTP), поэтому
подходит любой MCP-клиент: Claude Desktop и Claude Code, Cursor, VS Code, LM Studio, Jan и другие.

1. Включите **Настройки → MCP-сервер**.
2. Выберите формат и нажмите «Скопировать»:
   - **HTTP (JSON)** — для клиентов с удалёнными серверами: `{"mcpServers": {"shard": {"type": "http", "url": "…"}}}`;
   - **Claude Code** — `claude mcp add --transport http shard "…"`;
   - **stdio** — для клиентов, которые запускают только локальные команды, через `npx mcp-remote`
     или мост без зависимостей [`shard-mcp`](shard-mcp).

Сервер слушает только `127.0.0.1:8542` и работает, пока открыт Shard. В ссылке — токен доступа,
не показывайте её никому. Облачные коннекторы до локального адреса не достучатся.

## Приватность

- Вход только в свой аккаунт Microsoft (права `XboxLive.signin` и `offline_access`).
- Токены хранятся локально в хранилище учётных данных системы и отправляются только в сервисы Microsoft, Xbox Live и Minecraft.
- Лаунчер не изменяет игру и не обходит проверку владения. Офлайн-профили не обращаются к сервисам Mojang.

## CurseForge

В релизных сборках нет ключа CurseForge API. Чтобы искать на CurseForge, получите бесплатный ключ на
[console.curseforge.com](https://console.curseforge.com/) и вставьте его в **Настройки → CurseForge**.
Modrinth работает без ключа.

## Сборка из исходников

Нужны Node.js 20+, Rust (stable) и [зависимости Tauri](https://tauri.app/start/prerequisites/).
На Linux их поставит `./build.sh --deps` (Arch, Fedora, openSUSE, Ubuntu/Debian).

```bash
npm install
npm run tauri dev
```

На Windows `start.bat` запускает лаунчер в режиме разработки, а `build.bat` собирает установщик и портативный zip в `release/`.
На Linux `./build.sh` собирает AppImage, .deb и .rpm в `release/` (`./build.sh appimage` — только AppImage).

Необязательные файлы в `src-tauri/` (или переменные окружения):

| Файл | Переменная | Зачем |
|---|---|---|
| `msa-client-id.txt` | `SHARD_MSA_CLIENT_ID` | ID приложения Azure для входа через Microsoft |
| `curseforge-api-key.txt` | `SHARD_CURSEFORGE_API_KEY` | Ключ CurseForge для личной сборки (не коммитьте его) |
| `discord-client-id.txt` | `SHARD_DISCORD_CLIENT_ID` | ID приложения Discord для статуса |
| `update-repo.txt` | `SHARD_UPDATE_REPO` | Репозиторий GitHub, где лаунчер ищет обновления |

`SHARD_PUBLIC_BUILD=1` собирает без ключа CurseForge, даже если файл есть.

## Выпуск версии

Поднимите версию в `package.json`, `src-tauri/Cargo.toml` и `src-tauri/tauri.conf.json` и отправьте тег:

```bash
git tag v0.5.1
git push origin v0.5.1
```

GitHub Actions соберёт установщик и портативный zip для Windows, AppImage, .deb и .rpm для Linux и опубликует релиз.

## Лицензия

[MIT](LICENSE) © Asd1if. Не является официальным продуктом Minecraft. Не одобрено Mojang или Microsoft и не связано с ними.
