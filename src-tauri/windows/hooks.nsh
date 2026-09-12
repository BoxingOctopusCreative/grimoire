# Capture this file's directory at include time (macro expansion uses a different path).
!define GRIMOIRE_HOOK_DIR "${__FILEDIR__}"

Var GrimoireLibraryDir
Var GrimoireLibraryArgs

!macro NSIS_HOOK_POSTINSTALL
  ; Default ebook library location for new installs.
  StrCpy $GrimoireLibraryDir "$DOCUMENTS\Grimoire"
  StrCpy $GrimoireLibraryArgs '-LibraryPath "$GrimoireLibraryDir"'

  ; Optional silent / scripted override:
  ;   Grimoire_x64-setup.exe /LIBRARYPATH=D:\Books
  ${GetOptions} $CMDLINE "/LIBRARYPATH=" $0
  ${IfNot} ${Errors}
  ${AndIf} $0 != ""
    StrCpy $GrimoireLibraryDir $0
    StrCpy $GrimoireLibraryArgs '-LibraryPath "$GrimoireLibraryDir"'
  ${ElseIf} $PassiveMode != 1
  ${AndIfNot} ${Silent}
    ; Interactive installs can choose a different library folder.
    StrCpy $GrimoireLibraryArgs "-Prompt"
  ${EndIf}

  ; Pack and run the helper so JSON escaping stays correct for Windows paths.
  SetOutPath "$PLUGINSDIR"
  File "/oname=set-library-path.ps1" "${GRIMOIRE_HOOK_DIR}\set-library-path.ps1"
  nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$PLUGINSDIR\set-library-path.ps1" $GrimoireLibraryArgs'
  Pop $0
  DetailPrint "Grimoire library path setup exit code: $0"
!macroend
