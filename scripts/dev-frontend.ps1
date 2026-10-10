param(
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
Set-Location -LiteralPath $repoRoot

Write-Host "[myapp-frontend] repo: $repoRoot" -ForegroundColor Cyan

if ($DryRun) {
  Write-Host "[myapp-frontend] dry run: pnpm.cmd run dev:web" -ForegroundColor Yellow
  exit 0
}

Write-Host "[myapp-frontend] starting: pnpm.cmd run dev:web" -ForegroundColor Green
& pnpm.cmd run dev:web
exit $LASTEXITCODE
