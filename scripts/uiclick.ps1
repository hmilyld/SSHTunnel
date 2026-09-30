# UI Automation 辅助：按可访问名点击窗口内元素（WebView2 内容同样可见）
# 用法: .\scripts\uiclick.ps1 -WindowTitle "SSH 隧道管理器" -ElementName "新建转发"
param(
    [Parameter(Mandatory = $true)][string]$WindowTitle,
    [Parameter(Mandatory = $true)][string]$ElementName
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement

$winCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $WindowTitle)
$win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $winCond)
if (-not $win) { Write-Error "window not found: $WindowTitle"; exit 1 }

$elCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $ElementName)
$el = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $elCond)
if (-not $el) { Write-Error "element not found: $ElementName"; exit 2 }

$p = $null
if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) {
    $p.Invoke()
    Write-Output "invoked: $ElementName"
    exit 0
}
$lp = $null
if ($el.TryGetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern, [ref]$lp)) {
    $lp.DoDefaultAction()
    Write-Output "default-action: $ElementName"
    exit 0
}
Write-Error "no actionable pattern on: $ElementName"
exit 3
