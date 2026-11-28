!ifndef BUNDLE_DIR
  !error "BUNDLE_DIR define is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE define is required"
!endif
!ifndef RELEASE_VERSION
  !error "RELEASE_VERSION define is required"
!endif

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\oasis7"
InstallDirRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "InstallLocation"

!include "MUI2.nsh"

Name "oasis7"
OutFile "${OUT_FILE}"
BrandingText "oasis7"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\run-game.cmd"
!define MUI_FINISHPAGE_RUN_TEXT "Launch oasis7"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /r "${BUNDLE_DIR}\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateDirectory "$SMPROGRAMS\oasis7"
  CreateShortcut "$SMPROGRAMS\oasis7\oasis7.lnk" "$INSTDIR\run-game.cmd"
  CreateShortcut "$SMPROGRAMS\oasis7\Uninstall oasis7.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\oasis7.lnk" "$INSTDIR\run-game.cmd"

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "DisplayName" "oasis7"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "DisplayVersion" "${RELEASE_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "Publisher" "oasis7"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "DisplayIcon" "$INSTDIR\bin\oasis7_game_launcher.exe,0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$DESKTOP\oasis7.lnk"
  Delete "$SMPROGRAMS\oasis7\oasis7.lnk"
  Delete "$SMPROGRAMS\oasis7\Uninstall oasis7.lnk"
  RMDir "$SMPROGRAMS\oasis7"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\oasis7"
  RMDir /r "$INSTDIR"
SectionEnd
