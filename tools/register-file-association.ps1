<#
.SYNOPSIS
  Makes Windows open .oxicut project files with a built OxiCut, for this user only.

.DESCRIPTION
  The installer does this for real users (tauri.conf.json, bundle.fileAssociations). A copy of
  OxiCut that was only built, never installed, has no association, so double-clicking a project
  does nothing. This writes the per-user registry keys that give it one, and -Remove takes them
  away again. Nothing outside HKEY_CURRENT_USER\Software\Classes is touched, and no administrator
  rights are needed.

.EXAMPLE
  .\tools\register-file-association.ps1
  .\tools\register-file-association.ps1 -Exe D:\apps\oxicut-app.exe
  .\tools\register-file-association.ps1 -Remove
#>
[CmdletBinding()]
param(
    [string] $Exe = (Join-Path $PSScriptRoot '..\target\release\oxicut-app.exe'),
    [switch] $Remove
)

$ProgId = 'OxiCut.Project'
$Classes = 'HKCU:\Software\Classes'

function Update-Shell {
    # Tell Explorer the associations changed, so icons and "Open" show up without signing out.
    Add-Type -Namespace Win32 -Name Shell -MemberDefinition '[DllImport("shell32.dll")] public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);'
    [Win32.Shell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}

if ($Remove) {
    Remove-Item -Path "$Classes\.oxicut", "$Classes\$ProgId" -Recurse -ErrorAction SilentlyContinue
    Update-Shell
    Write-Host 'The .oxicut association is removed.'
    return
}

if (-not (Test-Path $Exe)) { throw "OxiCut was not found at $Exe. Build it first, or pass -Exe." }
$Exe = (Resolve-Path $Exe).Path

New-Item -Path "$Classes\.oxicut" -Value $ProgId -Force | Out-Null
New-Item -Path "$Classes\$ProgId" -Value 'OxiCut project' -Force | Out-Null
New-Item -Path "$Classes\$ProgId\DefaultIcon" -Value "`"$Exe`",0" -Force | Out-Null
New-Item -Path "$Classes\$ProgId\shell\open\command" -Value "`"$Exe`" `"%1`"" -Force | Out-Null
Update-Shell
Write-Host "Double-clicking a .oxicut file now opens it with $Exe."
Write-Host 'Undo with: .\tools\register-file-association.ps1 -Remove'
