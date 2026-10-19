$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$processes = @{}
foreach ($item in Get-CimInstance Win32_Process) { $processes[[int]$item.ProcessId] = $item }
$services = @{}
foreach ($item in Get-CimInstance Win32_Service -Filter 'ProcessId > 0') { $services[[int]$item.ProcessId] = $true }
$protected = @{}
$ancestor = [int]$env:MYAPP_PORT_MONITOR_PID
while ($ancestor -gt 0 -and -not $protected.ContainsKey($ancestor)) {
    $protected[$ancestor] = $true
    if (-not $processes.ContainsKey($ancestor)) { break }
    $ancestor = [int]$processes[$ancestor].ParentProcessId
}
$endpoints = @()
# Query all TCP rows first: a host with no listeners is a valid empty snapshot.
$endpoints += @(Get-NetTCPConnection | Where-Object State -eq Listen | ForEach-Object {
    [pscustomobject]@{ protocol = 'TCP'; address = [string]$_.LocalAddress; port = [int]$_.LocalPort; owner = [int]$_.OwningProcess }
})
$endpoints += @(Get-NetUDPEndpoint | ForEach-Object {
    [pscustomobject]@{ protocol = 'UDP'; address = [string]$_.LocalAddress; port = [int]$_.LocalPort; owner = [int]$_.OwningProcess }
})
$result = @(foreach ($group in ($endpoints | Group-Object owner)) {
    $ownerId = [int]$group.Name
    $item = $processes[$ownerId]
    $identity = ''
    $reason = ''
    $native = $null
    try {
        $native = [System.Diagnostics.Process]::GetProcessById($ownerId)
        $identity = $native.StartTime.ToFileTimeUtc().ToString([System.Globalization.CultureInfo]::InvariantCulture)
        # CIM metadata and process identity must refer to the same incarnation.
        if ($null -eq $item -or [Math]::Abs(($native.StartTime.ToUniversalTime() - $item.CreationDate.ToUniversalTime()).TotalMilliseconds) -gt 1) {
            $reason = 'Process changed during scan; refresh required'
            $identity = ''
        }
    } catch { $reason = 'Process exited or access denied' }
    finally { if ($null -ne $native) { $native.Dispose() } }
    $exe = if ($item) { [string]$item.ExecutablePath } else { '' }
    if ($ownerId -le 4 -or $services.ContainsKey($ownerId) -or ($exe -and $exe.StartsWith($env:SystemRoot + '\', [System.StringComparison]::OrdinalIgnoreCase))) {
        $reason = 'Windows process or system service'
    } elseif ($protected.ContainsKey($ownerId)) {
        $reason = 'This application or its parent process'
    } elseif (-not $exe -or -not $identity) {
        $reason = 'Process identity unavailable; refresh or check permissions'
    }
    [pscustomobject]@{
        pid = $ownerId
        name = $(if ($item) { [string]$item.Name } else { 'Unknown' })
        executable = $exe
        command_line = $(if ($item) { [string]$item.CommandLine } else { '' })
        parent_pid = $(if ($item) { [int]$item.ParentProcessId } else { 0 })
        started_at = $identity
        protected_reason = $reason
        endpoints = @($group.Group | Select-Object protocol, address, port | Sort-Object protocol, port, address -Unique)
    }
})
ConvertTo-Json -InputObject $result -Depth 5 -Compress
