@echo off
setlocal
cd /d "%~dp0"

where gradle >nul 2>nul || (
  echo [ERROR] Gradle was not found. Use GitHub Actions or install the developer prerequisites.
  exit /b 1
)
where cargo >nul 2>nul || (
  echo [ERROR] Rust Cargo was not found. Use GitHub Actions or install Rust.
  exit /b 1
)

call gradle -p template/android :app:assembleRelease --no-daemon || exit /b 1
copy /y "template\android\app\build\outputs\apk\release\app-release-unsigned.apk" "template\template.apk" >nul || exit /b 1
call cargo test || exit /b 1
call cargo build --release || exit /b 1

echo.
echo Completed: target\release\html2apk.exe
endlocal

