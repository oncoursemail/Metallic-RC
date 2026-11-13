$ErrorActionPreference = 'Stop'
$commit = '4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a'
$destination = Join-Path $PSScriptRoot 'Il2CppDumper-source'
$git = (Get-Command git -ErrorAction Stop).Source
if (Test-Path -LiteralPath $destination) {
    $head = & $git -C $destination rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $head.Trim() -ne $commit) { throw 'Existing upstream checkout is not the pinned commit; it was left unchanged.' }
    $dirty = & $git -C $destination status --porcelain
    if ($LASTEXITCODE -ne 0 -or $dirty) { throw 'Existing upstream checkout has local changes; it was left unchanged.' }
} else {
    & $git clone --no-checkout 'https://github.com/Perfare/Il2CppDumper.git' $destination
    if ($LASTEXITCODE -ne 0) { throw 'Upstream source acquisition failed.' }
    & $git -C $destination checkout --detach $commit
    if ($LASTEXITCODE -ne 0) { throw 'Pinned checkout failed.' }
}
Write-Output ('Pinned analysis source ready: ' + $destination)
Write-Output 'No game, captured binary, or reconstruction output has been executed.'
