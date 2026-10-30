; Tauri NSIS installer hooks: the uninstaller removes the install directory from
; PATH.
;
; The installer does not touch PATH, neither on install nor on an update (the
; updater runs this setup silently). The application adds the entry when the
; user clicks "Install command line tools" (ADR 0008). The uninstaller removes
; it, because the user may never start the application again; an earlier
; version of the installer added the same entry, so it is removed as well.
;
; Per-user install: HKCU\Environment\Path.
; Per-machine install: the machine PATH, only when the uninstaller is elevated
; and the install directory is under Program Files (where an earlier version
; added it). The value is kept as REG_EXPAND_SZ; other entries are never
; changed. Without the entry, PATH is not written at all.
;
; Tauri's NSIS build (stock NSIS 3.11 plus nsis_tauri_utils, no EnVar plugin, no
; large-strings build) caps strings at 1024 characters. ReadRegStr does not
; truncate a longer value: RegQueryValueEx fails with ERROR_MORE_DATA, NSIS
; returns an EMPTY string and sets the error flag (the same happens for a value
; that is not a string). An empty read is therefore ambiguous: the value is
; missing, or it exists and could not be read. The hook tells them apart with
; EnumRegValue; the uninstaller never writes a value it could not read, because
; writing over it would drop every other entry.
;
; Every case that leaves PATH untouched says why in the details log.

!include "LogicLib.nsh"
!include "WinMessages.nsh" ; HWND_BROADCAST, WM_SETTINGCHANGE

!define AGENTLENS_MACHINE_ENV "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"

; Removes the install directory from the PATH entry list of one registry root.
;   Push root ("HKCU" or "HKLM"), Call un.AgentLensRemovePath.
; Only the uninstaller calls it (NSIS only lets uninstaller sections call un.
; functions). Entries are compared exactly (case-insensitive, as Windows does).
; All registers it uses are restored.
Function un.AgentLensRemovePath
  Exch $R9 ; root
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  Push $7

  ; an entry with ";" would split into two, "%" would be expanded
  StrCpy $5 0
  ${Do}
    StrCpy $6 $INSTDIR 1 $5
    ${If} $6 == ""
      ${Break}
    ${EndIf}
    ${If} $6 == ";"
    ${OrIf} $6 == "%"
      DetailPrint "AgentLens: PATH not changed, the install directory contains ';' or '%': $INSTDIR"
      Goto done
    ${EndIf}
    IntOp $5 $5 + 1
  ${Loop}

  ${If} $R9 == "HKLM"
    UserInfo::GetAccountType
    Pop $0
    ${If} $0 != "Admin"
      DetailPrint "AgentLens: machine PATH not changed, the uninstaller is not elevated"
      Goto done
    ${EndIf}
    ; an earlier version added the machine entry only for a directory under
    ; Program Files, which other users cannot write to
    StrCpy $3 0
    ${ForEach} $1 0 1 + 1
      ${If} $1 == 0
        StrCpy $2 $PROGRAMFILES
      ${Else}
        StrCpy $2 $PROGRAMFILES64
      ${EndIf}
      StrLen $5 $2
      StrCpy $6 $INSTDIR $5
      StrCpy $7 $INSTDIR 1 $5
      ${If} $2 != ""
      ${AndIf} $6 == $2
      ${AndIf} $7 == "\"
        StrCpy $3 1
      ${EndIf}
    ${Next}
    ${If} $3 == 0
      DetailPrint "AgentLens: machine PATH not changed, the install directory is not under Program Files: $INSTDIR"
      Goto done
    ${EndIf}
    ClearErrors
    ReadRegStr $0 HKLM "${AGENTLENS_MACHINE_ENV}" "Path"
  ${Else}
    ClearErrors
    ReadRegStr $0 HKCU "Environment" "Path"
  ${EndIf}

  ; An empty read with the error flag set: missing value or unreadable value
  ${If} ${Errors}
    StrCpy $3 0 ; 1 = a value named Path exists
    StrCpy $4 0 ; index of the value name
    ${Do}
      ClearErrors
      ${If} $R9 == "HKLM"
        EnumRegValue $5 HKLM "${AGENTLENS_MACHINE_ENV}" $4
      ${Else}
        EnumRegValue $5 HKCU "Environment" $4
      ${EndIf}
      ${If} ${Errors}
        ${Break}
      ${EndIf}
      ${If} $5 == "Path"
        StrCpy $3 1
        ${Break}
      ${EndIf}
      IntOp $4 $4 + 1
    ${Loop}
    ClearErrors
    ${If} $3 == 1
      DetailPrint "AgentLens: PATH not changed, the value exists but could not be read (longer than 1023 characters or not a text value)"
      Goto done
    ${EndIf}
    DetailPrint "AgentLens: PATH not changed, there is no PATH value"
    Goto done
  ${EndIf}

  ; 1023 is the longest string NSIS holds; a value this long may be cut off
  StrLen $7 $0
  ${If} $7 >= 1023
    DetailPrint "AgentLens: PATH not changed, it is too long to edit safely ($7 characters)"
    Goto done
  ${EndIf}

  ; $0 = old value, $1 = remaining text, $2 = rebuilt value, $3 = found flag
  StrCpy $1 $0
  StrCpy $2 ""
  StrCpy $3 0
  ${While} $1 != ""
    ; split off the next entry into $4
    StrCpy $5 0
    ${Do}
      StrCpy $6 $1 1 $5
      ${If} $6 == ""
      ${OrIf} $6 == ";"
        ${Break}
      ${EndIf}
      IntOp $5 $5 + 1
    ${Loop}
    StrCpy $4 $1 $5
    IntOp $5 $5 + 1
    StrCpy $1 $1 "" $5

    ${If} $4 == ""
      ; skip empty segments
    ${ElseIf} $4 == $INSTDIR
      ; found: left out of the rebuilt value
      StrCpy $3 1
    ${Else}
      ${If} $2 == ""
        StrCpy $2 $4
      ${Else}
        StrCpy $2 "$2;$4"
      ${EndIf}
    ${EndIf}
  ${EndWhile}

  ${If} $3 == 0
    DetailPrint "AgentLens: PATH not changed, it has no entry for $INSTDIR"
    Goto done
  ${EndIf}

  ${If} $R9 == "HKLM"
    WriteRegExpandStr HKLM "${AGENTLENS_MACHINE_ENV}" "Path" $2
  ${Else}
    WriteRegExpandStr HKCU "Environment" "Path" $2
  ${EndIf}

  done:
  Pop $7
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
  Pop $R9
FunctionEnd

!macro AgentLensRemovePathCall root
  Push "${root}"
  Call un.AgentLensRemovePath
!macroend

; Per-user install touches HKCU; per-machine install touches HKLM.
; $MultiUser.InstallMode exists only in the template's "both" install mode
; (MultiUser.nsh is not included otherwise), so the mode of the bundle
; (${INSTALLMODE}: currentUser, perMachine or both) decides at compile time.
; The template defines INSTALLMODE after it includes this file, which is fine:
; a macro body is expanded where it is inserted.
!macro AgentLensRemovePath
  !if "${INSTALLMODE}" == "perMachine"
    !insertmacro AgentLensRemovePathCall "HKLM"
  !else if "${INSTALLMODE}" == "both"
    ${If} $MultiUser.InstallMode == "AllUsers"
      !insertmacro AgentLensRemovePathCall "HKLM"
    ${Else}
      !insertmacro AgentLensRemovePathCall "HKCU"
    ${EndIf}
  !else
    !insertmacro AgentLensRemovePathCall "HKCU"
  !endif
  ; tell running programs (Explorer, new terminals) that the environment changed
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

; No NSIS_HOOK_POSTINSTALL: the template inserts a hook only when it is defined,
; so install and update leave PATH alone.

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro AgentLensRemovePath
!macroend
