; Tauri NSIS installer hooks: put the install directory on PATH so that
; `agentlens` and `agentlens-mcp` are found in a new terminal.
;
; Per-user install: HKCU\Environment\Path (no admin rights needed).
; Per-machine install: the machine PATH, only when the installer is elevated.
; The value is kept as REG_EXPAND_SZ; other entries are never changed.

!include "LogicLib.nsh"

; Edits the PATH entry list of one registry root.
;   Push mode ("add" or "remove"), push root ("HKCU" or "HKLM"), Call.
; Entries are compared exactly (case-insensitive, as Windows does).
Function AgentLensEditPath
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

  ${If} $R9 == "HKLM"
    ReadRegStr $0 HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path"
  ${Else}
    ReadRegStr $0 HKCU "Environment" "Path"
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
      ${If} $R8 != "remove"
      ${AndIf} $R8 != "remove"
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
      StrCpy $2 "$2;$INSTDIR"
    ${EndIf}
    StrCpy $3 2 ; changed
  ${ElseIf} $R8 == "remove"
  ${AndIf} $3 == 1
    StrCpy $3 2 ; changed
  ${EndIf}

  ${If} $3 == 2
    ${If} $R9 == "HKLM"
      WriteRegExpandStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "Path" $2
    ${Else}
      WriteRegExpandStr HKCU "Environment" "Path" $2
    ${EndIf}
  ${EndIf}

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

; Per-user install touches HKCU; per-machine install touches HKLM, and only
; when elevated.
!macro AgentLensPath mode
  ${If} $MultiUser.InstallMode == "AllUsers"
    UserInfo::GetAccountType
    Pop $0
    ${If} $0 == "Admin"
      Push "${mode}"
      Push "HKLM"
      Call AgentLensEditPath
    ${EndIf}
  ${Else}
    Push "${mode}"
    Push "HKCU"
    Call AgentLensEditPath
  ${EndIf}
  ; tell running programs (Explorer, new terminals) that the environment changed
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro AgentLensPath "add"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro AgentLensPath "remove"
!macroend
