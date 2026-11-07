# Shard Launcher

Бесплатный лаунчер с открытым кодом для **Minecraft: Java Edition** на Tauri, Rust и React.
Сначала Windows, Linux и macOS — в планах.

[English](README.md) · **Русский**

## Скачать

Установщик и портативная версия — в [Releases](https://github.com/Asd1if/Shard-Launcher/releases).
Лаунчер сам обновляется, когда выходит новая версия.

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

```bash
npm install
npm run tauri dev
```

На Windows `start.bat` запускает лаунчер в режиме разработки, а `build.bat` собирает установщик и портативный zip в `release/`.

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

GitHub Actions соберёт установщик и портативный zip и опубликует релиз.

## Лицензия

[MIT](LICENSE) © Asd1if. Не является официальным продуктом Minecraft. Не одобрено Mojang или Microsoft и не связано с ними.
