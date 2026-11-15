; Detect the external Microsoft x64 runtime before replacing an existing installation.
!define OCRTRANS_VCREDIST_MINOR 44
!define OCRTRANS_VCREDIST_KEY "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64"

; $0 = 1 if a runtime ≥ 14.${OCRTRANS_VCREDIST_MINOR} is installed (64-bit or 32-bit registry view), else 0
!macro OCRTRANS_VCREDIST_PRESENT
  Push $1
  Push $2
  StrCpy $0 0
  ReadRegDWORD $1 HKLM64 "${OCRTRANS_VCREDIST_KEY}" "Installed"
  ReadRegDWORD $2 HKLM64 "${OCRTRANS_VCREDIST_KEY}" "Minor"
  ${If} $1 == 1
  ${AndIf} $2 >= ${OCRTRANS_VCREDIST_MINOR}
    StrCpy $0 1
  ${EndIf}
  ReadRegDWORD $1 HKLM32 "${OCRTRANS_VCREDIST_KEY}" "Installed"
  ReadRegDWORD $2 HKLM32 "${OCRTRANS_VCREDIST_KEY}" "Minor"
  ${If} $1 == 1
  ${AndIf} $2 >= ${OCRTRANS_VCREDIST_MINOR}
    StrCpy $0 1
  ${EndIf}
  Pop $2
  Pop $1
!macroend

!macro NSIS_HOOK_PREINSTALL
  Push $0
  !insertmacro OCRTRANS_VCREDIST_PRESENT
  ${If} $0 != 1
    MessageBox MB_YESNO|MB_ICONINFORMATION "请先安装微软 Visual C++ x64 运行库，再重新运行 OCR Trans 安装程序。打开微软官方下载页？ / Install the Microsoft Visual C++ x64 runtime, then run setup again. Open the official download page?" /SD IDNO IDNO ocrtrans_vcredist_abort
    ExecShell "open" "https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist"
    ocrtrans_vcredist_abort:
    Pop $0
    SetErrorLevel 1603
    Abort "Microsoft Visual C++ x64 runtime required (1603)."
  ${EndIf}
  Pop $0
!macroend
