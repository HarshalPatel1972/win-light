; Matchstick used to be called AnCheck. Installing Matchstick replaces it:
; if an AnCheck install is found, its own uninstaller is run first, so nobody
; ends up with two launchers. (Its index is imported by the app on first run.)

!macro RemoveLegacyAnCheck ROOT
  ReadRegStr $R0 ${ROOT} "Software\Microsoft\Windows\CurrentVersion\Uninstall\AnCheck" "InstallLocation"
  ${If} $R0 != ""
    ; The location is stored in quotes; take them off
    StrCpy $R1 $R0 1
    ${If} $R1 == '"'
      StrCpy $R0 $R0 "" 1
      StrCpy $R0 $R0 -1
    ${EndIf}
    ${If} ${FileExists} "$R0\uninstall.exe"
      DetailPrint "Removing AnCheck, which Matchstick replaces..."
      ; Make sure the old app is not running, then uninstall it silently.
      ; ExecShellWait lets Windows ask for permission if it was installed for all users.
      nsExec::Exec 'taskkill /F /IM ancheck.exe'
      Pop $R1
      ExecShellWait "" "$R0\uninstall.exe" "/S"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  Push $R0
  Push $R1
  SetRegView 64
  !insertmacro RemoveLegacyAnCheck HKLM
  !insertmacro RemoveLegacyAnCheck HKCU
  Pop $R1
  Pop $R0
!macroend
