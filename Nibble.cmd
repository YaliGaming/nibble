@echo off
if not exist "%~dp0node_modules\electron\dist\electron.exe" (
  echo Getting Electron, one time only...
  pushd "%~dp0"
  call npm install --no-fund --no-audit
  call node node_modules\electron\install.js
  popd
)
start "" "%~dp0node_modules\electron\dist\electron.exe" "%~dp0."
