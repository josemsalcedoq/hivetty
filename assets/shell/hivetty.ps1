# hivetty shell integration for PowerShell 5.1 and 7: prompt marks (OSC 133) and the current
# directory (OSC 7, plus OSC 9;9 for tools that read the ConEmu form).
# hivetty dot-sources this after the user's profile.

if ($global:__HivettyLoaded) { return }
$global:__HivettyLoaded = $true
$global:__HivettyOriginalPrompt = $function:prompt
$global:__HivettyLastHistoryId = -1

function global:prompt {
    # Read the status of the last command before anything else changes it.
    $succeeded = $?
    $exitCode = $global:LASTEXITCODE
    $esc = [char]27
    $marks = ''

    $last = Get-History -Count 1
    if ($global:__HivettyLastHistoryId -eq -1) {
        # First prompt: the history so far (including the line that loaded this script) is
        # not a command the user ran in this pane.
        $global:__HivettyLastHistoryId = if ($null -ne $last) { $last.Id } else { 0 }
    } elseif ($null -ne $last -and $last.Id -ne $global:__HivettyLastHistoryId) {
        $global:__HivettyLastHistoryId = $last.Id
        $status = if ($succeeded) { 0 } elseif ($exitCode) { $exitCode } else { 1 }
        $marks += "$esc]133;D;$status$esc\"
    }

    $location = $executionContext.SessionState.Path.CurrentLocation
    if ($location.Provider.Name -eq 'FileSystem') {
        $path = $location.ProviderPath -replace '\\', '/'
        if (-not $path.StartsWith('/')) { $path = "/$path" }
        $marks += "$esc]7;file://$([Environment]::MachineName)$([Uri]::EscapeUriString($path))$esc\"
        $marks += "$esc]9;9;`"$($location.ProviderPath)`"$esc\"
    }
    $marks += "$esc]133;A$esc\"

    $text = & $global:__HivettyOriginalPrompt
    $global:LASTEXITCODE = $exitCode
    return "$marks$text$esc]133;B$esc\"
}

# Mark where command output starts (133;C) when Enter accepts the line. Only if the user has
# not bound Enter to something of their own.
if (Get-Module -Name PSReadLine) {
    $enter = Get-PSReadLineKeyHandler -Bound | Where-Object { $_.Key -eq 'Enter' }
    if ($null -eq $enter -or $enter.Function -eq 'AcceptLine') {
        Set-PSReadLineKeyHandler -Chord Enter -ScriptBlock {
            [Microsoft.PowerShell.PSConsoleReadLine]::AcceptLine()
            [Console]::Write("$([char]27)]133;C$([char]27)\")
        }
    }
}
