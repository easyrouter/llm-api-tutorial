; NSIS hooks for the Tauri installer (bundle.windows.nsis.installerHooks).
;
; The bundler !includes this file near the top of installer.nsi, BEFORE it defines
; ${MANUFACTURER} / ${PRODUCTNAME}, so the registry paths below are spelled out literally.
; scripts/check-nsis-hooks.mjs fails `npm run check` when they drift from tauri.conf.json.
;
; Why this exists: Tauri's reinstall page runs the previous uninstaller as
;   "<old dir>\uninstall.exe" /P _?=$4
; where $4 is read from HKCU\Software\<publisher>\<product>. Builds made before
; bundle.publisher was set wrote that key under the fallback publisher "company", so a new
; installer finds nothing, passes an empty `_?=` and the old uninstaller dies with
; "NSIS Error: Error launching installer" (NSIS exits on an invalid `_?=` path).
; SeedRouterGuiInit runs before any page is shown and recovers the install directory from the
; legacy key or, failing that, from the uninstall string itself.

!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define SR_MANUPRODUCTKEY "Software\SeedRouter\SeedRouter Onboarding"
!define SR_LEGACY_MANUPRODUCTKEY "Software\company\SeedRouter Onboarding"
!define SR_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SeedRouter Onboarding"

!define MUI_CUSTOMFUNCTION_GUIINIT SeedRouterGuiInit

Function SeedRouterGuiInit
  Push $0
  Push $1
  Push $2

  ReadRegStr $0 SHCTX "${SR_MANUPRODUCTKEY}" ""
  ${If} $0 != ""
    Goto done ; current key present — nothing to recover
  ${EndIf}

  ; 1. Key written by builds whose publisher fell back to "company".
  ReadRegStr $0 SHCTX "${SR_LEGACY_MANUPRODUCTKEY}" ""

  ; 2. Parent directory of the registered uninstaller ("C:\...\uninstall.exe", quoted).
  ${If} $0 == ""
    ReadRegStr $1 SHCTX "${SR_UNINSTKEY}" "UninstallString"
    ${If} $1 != ""
      StrCpy $2 $1 1
      ${If} $2 == '"'
        StrCpy $1 $1 "" 1
        StrCpy $2 $1 1 -1
        ${If} $2 == '"'
          StrCpy $1 $1 -1
        ${EndIf}
      ${EndIf}
      ${GetParent} $1 $0
    ${EndIf}
  ${EndIf}

  ${If} $0 != ""
  ${AndIf} ${FileExists} "$0\uninstall.exe"
    WriteRegStr SHCTX "${SR_MANUPRODUCTKEY}" "" $0
    ; Mirror RestorePreviousInstallLocation (.onInit ran before we could fix the key).
    StrCpy $INSTDIR $0
  ${EndIf}

done:
  Pop $2
  Pop $1
  Pop $0
FunctionEnd
