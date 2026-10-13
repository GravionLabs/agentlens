; Tauri NSIS installer hooks: put the install directory on PATH so that
; `agentlens` and `agentlens-mcp` are found in a new terminal.
;
; Per-user install: HKCU\Environment\Path (no admin rights needed).
; Per-machine install: the machine PATH, only when the installer is elevated
; and the install directory is under Program Files (writable by admins only).
; The value is kept as REG_EXPAND_SZ; other entries are never changed.
;
; Tauri's NSIS build (stock NSIS 3.11 plus nsis_tauri_utils, no EnVar plugin, no
; large-strings build) caps strings at 1024 characters, and ReadRegStr truncates
; silently. Writing a truncated PATH back would destroy entries, so PATH is left
; untouched when the old value is near the cap or the new one would exceed it.
;
; Every case that leaves PATH untouched says why in the details log.

!include "LogicLib.nsh"
!include "WinMessages.nsh" ; HWND_BROADCAST, WM_SETTINGCHANGE

!define AGENTLENS_MACHINE_ENV "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"

; Defines ${un}AgentLensEditPath, once for the installer ("") and once for the
; uninstaller ("un."): NSIS only lets uninstaller sections call un. functions.
;
; Edits the PATH entry list of one registry root.
;   Push mode ("add" or "remove"), push root ("HKCU" or "HKLM"), Call.
; Entries are compared exactly (case-insensitive, as Windows does).
; All registers it uses are restored.
!macro AgentLensDefineEditPath un
Function ${un}AgentLensEditPath
  Exch $R9 ; root
  Exch
  Exch $R8 ; mode
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
      DetailPrint "AgentLens: machine PATH not changed, the installer is not elevated"
      Goto done
    ${EndIf}
    ; a directory outside Program Files may be writable by other users, who
    ; could then plant programs on every user's PATH
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
    ReadRegStr $0 HKLM "${AGENTLENS_MACHINE_ENV}" "Path"
  ${Else}
    ReadRegStr $0 HKCU "Environment" "Path"
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
      StrCpy $3 1
      ${If} $R8 != "remove"
        ${If} $2 == ""
          StrCpy $2 $4
        ${Else}
          StrCpy $2 "$2;$4"
        ${EndIf}
      ${EndIf}
    ${Else}
      ${If} $2 == ""
        StrCpy $2 $4
      ${Else}
        StrCpy $2 "$2;$4"
      ${EndIf}
    ${EndIf}
  ${EndWhile}

  ${If} $R8 == "add"
  ${AndIf} $3 == 0
    ${If} $2 == ""
      StrCpy $2 $INSTDIR
    ${Else}
      ; the new value must still fit into one NSIS string
      StrLen $7 $2
      StrLen $5 $INSTDIR
      IntOp $7 $7 + $5
      IntOp $7 $7 + 1
      ${If} $7 > 1023
        DetailPrint "AgentLens: PATH not changed, the new value would be too long ($7 characters)"
        Goto done
      ${EndIf}
      StrCpy $2 "$2;$INSTDIR"
    ${EndIf}
    StrCpy $3 2 ; changed
  ${ElseIf} $R8 == "remove"
  ${AndIf} $3 == 1
    StrCpy $3 2 ; changed
  ${EndIf}

  ${If} $3 == 2
    ${If} $R9 == "HKLM"
      WriteRegExpandStr HKLM "${AGENTLENS_MACHINE_ENV}" "Path" $2
    ${Else}
      WriteRegExpandStr HKCU "Environment" "Path" $2
    ${EndIf}
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
  Pop $R8
  Pop $R9
FunctionEnd
!macroend

!insertmacro AgentLensDefineEditPath ""
!insertmacro AgentLensDefineEditPath "un."

!macro AgentLensEditPathCall un mode root
  Push "${mode}"
  Push "${root}"
  Call ${un}AgentLensEditPath
!macroend

; Per-user install touches HKCU; per-machine install touches HKLM.
; $MultiUser.InstallMode exists only in the template's "both" install mode
; (MultiUser.nsh is not included otherwise), so the mode of the bundle
; (${INSTALLMODE}: currentUser, perMachine or both) decides at compile time.
; The template defines INSTALLMODE after it includes this file, which is fine:
; a macro body is expanded where it is inserted.
!macro AgentLensPath un mode
  !if "${INSTALLMODE}" == "perMachine"
    !insertmacro AgentLensEditPathCall "${un}" "${mode}" "HKLM"
  !else if "${INSTALLMODE}" == "both"
    ${If} $MultiUser.InstallMode == "AllUsers"
      !insertmacro AgentLensEditPathCall "${un}" "${mode}" "HKLM"
    ${Else}
      !insertmacro AgentLensEditPathCall "${un}" "${mode}" "HKCU"
    ${EndIf}
  !else
    !insertmacro AgentLensEditPathCall "${un}" "${mode}" "HKCU"
  !endif
  ; tell running programs (Explorer, new terminals) that the environment changed
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro AgentLensPath "" "add"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro AgentLensPath "un." "remove"
!macroend
