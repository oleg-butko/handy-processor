# Handy Processor Launcher

Опциональная Tauri-оболочка для запуска Handy Processor в system tray. Сам
Node SEA остаётся самостоятельным приложением и может запускаться без этого
лаунчера.

## Подготовка sidecar

Сначала соберите SEA, затем скопируйте его под именем, которое ожидает Tauri:

```powershell
npm run build:sea:node26
npm run stage:launcher:sidecar
```

Target triple обычно определяется через `rustc`. Его и путь к готовому SEA
можно передать явно:

```powershell
node scripts/stage-launcher-sidecar.mjs `
  x86_64-pc-windows-msvc `
  sea-build/handy-processor.exe
```

## Проверка каркаса

```powershell
npm run install:launcher
npm run check:launcher
```

Полная инструкция по сборке Windows portable release будет добавлена после
реализации tray и управления процессом.
