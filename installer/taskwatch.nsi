; RuskWatcher setup (NSIS). Build with installer\build.ps1.
; /DVERSION=x.y.z /DSRC=<dir with RuskWatcher.exe + service.exe> /DOUT=<setup exe path> /DICON=<ico>
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif

Name "RuskWatcher"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\RuskWatcher"
InstallDirRegKey HKLM "Software\RuskWatcher" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_COMPONENTSPAGE_TEXT_TOP "Choose how RuskWatcher should run in the background."
!define MUI_FINISHPAGE_RUN "$INSTDIR\RuskWatcher.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open RuskWatcher"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "RuskWatcher (required)" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"

  ; upgrade: stop and remove the old service so files are not locked
  IfFileExists "$INSTDIR\service.exe" 0 +3
    nsExec::Exec '"$INSTDIR\service.exe" uninstall'
    Sleep 1500

  File "/oname=RuskWatcher.exe" "${SRC}\RuskWatcher.exe"
  File "${SRC}\service.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; collector runs as a LocalSystem service; manual start unless the option below is ticked
  nsExec::ExecToLog '"$INSTDIR\service.exe" install manual'
  nsExec::ExecToLog '"$INSTDIR\service.exe" start'

  CreateDirectory "$SMPROGRAMS\RuskWatcher"
  CreateShortCut "$SMPROGRAMS\RuskWatcher\RuskWatcher.lnk" "$INSTDIR\RuskWatcher.exe"

  WriteRegStr HKLM "Software\RuskWatcher" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher" "DisplayName" "RuskWatcher"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher" "DisplayIcon" "$INSTDIR\RuskWatcher.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher" "NoModify" 1
SectionEnd

Section "Start collecting automatically when Windows starts" SecAuto
  nsExec::ExecToLog '"$INSTDIR\service.exe" autostart on'
SectionEnd

LangString DESC_Main ${LANG_ENGLISH} "The dashboard app and the background collector service."
LangString DESC_Auto ${LANG_ENGLISH} "Without this, collection only runs after you press Start in Settings. Can be changed later in RuskWatcher > Settings."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecMain} $(DESC_Main)
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAuto} $(DESC_Auto)
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  nsExec::Exec '"$INSTDIR\service.exe" uninstall'
  Sleep 1500
  Delete "$INSTDIR\RuskWatcher.exe"
  Delete "$INSTDIR\service.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\RuskWatcher\RuskWatcher.lnk"
  RMDir "$SMPROGRAMS\RuskWatcher"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RuskWatcher"
  DeleteRegKey HKLM "Software\RuskWatcher"
  ; collected history in %ProgramData%\RuskWatcher is kept on purpose
SectionEnd
