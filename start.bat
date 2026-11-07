@echo off
rem Запуск Shard в режиме разработки (окно лаунчера + горячая перезагрузка интерфейса).
rem Файл в кодировке CP866 с переводами строк CRLF: иначе cmd путает русский текст с командами.
cd /d "%~dp0"

where npm >nul 2>nul || (
  echo Не найден Node.js. Установите его с https://nodejs.org и запустите снова.
  pause
  exit /b 1
)
where cargo >nul 2>nul || (
  echo Не найден Rust. Установите его с https://rustup.rs и запустите снова.
  pause
  exit /b 1
)

rem Порт 1420 нужен интерфейсу. Если он занят, скорее всего Shard уже запущен в другом окне.
netstat -ano | findstr /r /c:":1420 .*LISTENING" >nul && (
  echo Порт 1420 занят - похоже, Shard уже запущен. Закройте другое окно запуска и попробуйте снова.
  echo Процесс на порту:
  netstat -ano | findstr /r /c:":1420 .*LISTENING"
  pause
  exit /b 1
)

if not exist node_modules (
  echo Устанавливаю зависимости...
  call npm install || (pause & exit /b 1)
)

echo Запускаю Shard. Первая сборка Rust занимает несколько минут.
call npm run tauri dev
if errorlevel 1 pause
