# NOVA Local Build and Verification Script
# Automates clean formatting check, clippy checks, build compilation, and unit/integration testing.

$ErrorActionPreference = "Stop"

Write-Host "=============================================" -ForegroundColor Cyan
Write-Host "        NOVA Build & Test Automation         " -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan

# 1. Format check
Write-Host "`n[1/4] Checking code formatting (cargo fmt)..." -ForegroundColor Yellow
& cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) {
    Write-Host "[-] Formatting check failed. Run 'cargo fmt' locally to fix." -ForegroundColor Red
    exit 1
}
Write-Host "[+] Formatting check passed cleanly." -ForegroundColor Green

# 2. Clippy static analysis
Write-Host "`n[2/4] Running static analysis linter (clippy)..." -ForegroundColor Yellow
& cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) {
    Write-Host "[-] Clippy linter found warnings/errors." -ForegroundColor Red
    exit 1
}
Write-Host "[+] Clippy check completed with zero errors." -ForegroundColor Green

# 3. Compilation build
Write-Host "`n[3/4] Compiling workspace targets..." -ForegroundColor Yellow
& cargo build --all-targets
if ($LASTEXITCODE -ne 0) {
    Write-Host "[-] Cargo compilation build failed." -ForegroundColor Red
    exit 1
}
Write-Host "[+] Workspace compiled successfully." -ForegroundColor Green

# 4. Run tests
Write-Host "`n[4/4] Running test suites (cargo test)..." -ForegroundColor Yellow
& cargo test --workspace
if ($LASTEXITCODE -ne 0) {
    Write-Host "[-] One or more tests failed." -ForegroundColor Red
    exit 1
}
Write-Host "[+] All unit and integration tests passed successfully!" -ForegroundColor Green

Write-Host "`n=============================================" -ForegroundColor Cyan
Write-Host "      NOVA Local Verification Succeeded       " -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan
