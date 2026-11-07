@echo off
rem Сборка установщика Shard для Windows (NSIS, .exe).
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

rem Client ID Microsoft вшивается в программу при сборке. Без него Shard работает, но без входа Microsoft.
call :check_key src-tauri\msa-client-id.txt "Client ID Microsoft" "вход через Microsoft будет недоступен"
rem Публичная сборка - без ключа CurseForge: пользователи вводят свой ключ в настройках.
set "SHARD_PUBLIC_BUILD=1"

if not exist node_modules (
  echo Устанавливаю зависимости...
  call npm install || (pause & exit /b 1)
)

echo.
echo Собираю Shard. Сборка с оптимизацией занимает 5-15 минут.
call npm run tauri build || (
  echo.
  echo Сборка не удалась - ошибка выше.
  pause
  exit /b 1
)

if not exist release mkdir release
set "FOUND="
for %%f in (src-tauri\target\release\bundle\nsis\*-setup.exe) do (
  copy /y "%%f" release\ >nul
  set "FOUND=%%~nxf"
)
if not defined FOUND (
  echo Установщик не найден в src-tauri\target\release\bundle\nsis.
  pause
  exit /b 1
)

rem Портативная версия: тот же exe, но данные лежат рядом с ним в папке data
set "PORTABLE=release\Shard-portable"
if exist "%PORTABLE%" rmdir /s /q "%PORTABLE%"
mkdir "%PORTABLE%"
copy /y src-tauri\target\release\shard.exe "%PORTABLE%\Shard.exe" >nul
> "%PORTABLE%\portable.txt" echo Portable mode: Shard keeps its data in the "data" folder next to Shard.exe. Delete this file to use %%APPDATA%% instead.
if exist release\Shard-portable.zip del release\Shard-portable.zip
powershell -NoProfile -Command "Compress-Archive -Path '%PORTABLE%' -DestinationPath 'release\Shard-portable.zip'" || (
  echo Не удалось упаковать портативную версию в zip.
)

echo.
echo Готово:
echo   установщик:            release\%FOUND%
echo   портативная версия:    release\Shard-portable\Shard.exe  (и архив release\Shard-portable.zip)
explorer release
pause
exit /b 0

:check_key
if not exist "%~1" goto key_missing
for %%s in ("%~1") do if %%~zs==0 goto key_missing
exit /b 0
:key_missing
echo Внимание: %~2 не задан (%~1) - %~3.
exit /b 0
