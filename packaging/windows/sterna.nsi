; The Windows installer. Assembled by `build.sh`, which cross-compiles the
; shell, stages every file beside it and generates the two file lists this
; includes — so nothing here has to be edited when a dependency of Qt's
; changes. Run `makensis` through that script rather than directly; the
; -D definitions below have no useful defaults.
;
; **The stub is amd64, not the customary x86.** An x86 installer is the
; convention and runs anywhere, but it costs two things here and buys nothing:
; a 32-bit process writing HKLM\Software lands in Wow6432Node unless every
; write is wrapped in `SetRegView 64`, and — the reason that settled it — the
; only Wine in this development environment is 64-bit with no WOW64, so an x86
; stub cannot be started at all before it is shipped. A release artifact that
; cannot be run before release is the wrong trade for supporting a 32-bit
; Windows that could not run the program inside it either. What it costs: on
; 32-bit Windows the refusal comes from Windows ("this app can't run on your
; PC") rather than from a message of ours.

Target amd64-unicode
ManifestDPIAware true
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "Sections.nsh"

!ifndef VERSION
  !error "VERSION is not defined — run build.sh, not makensis"
!endif
!ifndef STAGE
  !error "STAGE is not defined — run build.sh, not makensis"
!endif
!ifndef FILES_NSH
  !error "FILES_NSH is not defined — run build.sh, not makensis"
!endif
!ifndef UNINSTALL_NSH
  !error "UNINSTALL_NSH is not defined — run build.sh, not makensis"
!endif
!ifndef OUTFILE
  !error "OUTFILE is not defined — run build.sh, not makensis"
!endif

!define NAME "Sterna"
!define PUBLISHER "The Sterna authors"
!define URL "https://github.com/nataloko/Sterna"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${NAME}"
!define PROGID "Sterna.MacroFile"

Var UpdateRestart
Var UpdateInstaller

Name "${NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${NAME}"
InstallDirRegKey HKLM "Software\${NAME}" "InstallDir"
RequestExecutionLevel admin

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}.0"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "The Sterna authors"
VIAddVersionKey "FileDescription" "${NAME} setup"

; --- pages -------------------------------------------------------------------

!define MUI_ICON "..\..\assets\branding\sterna\sterna.ico"
!define MUI_UNICON "..\..\assets\branding\sterna\sterna.ico"
!define MUI_ABORTWARNING

; Sterna's own licence is 3-clause BSD, and it is not the only licence in the
; installed tree: Qt is bundled and is LGPLv3. The page shows ours and says
; where the rest are, which is the same division the AppImage makes.
!define MUI_LICENSEPAGE_TEXT_BOTTOM "Sterna itself is under the licence above. It bundles Qt, which is LGPLv3 — the text, and how to substitute your own build of it, are installed in the doc folder."
!define MUI_LICENSEPAGE_BUTTON "Next >"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${STAGE}\doc\LICENSE.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; **The finish page must not start the program itself.** This installer asks
; for administrator rights, so anything it runs inherits them — and Sterna's
; settings live under the *running user's* AppData, so a first run as
; Administrator writes `sterna.ini` into the administrator's profile and the
; user's own later runs start from defaults, permanently and with nothing to
; see. Going through Explorer, which is already running as the user, hands the
; program back its proper token.
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Start ${NAME}"
!define MUI_FINISHPAGE_RUN_FUNCTION StartSterna
!define MUI_FINISHPAGE_LINK "${URL}"
!define MUI_FINISHPAGE_LINK_LOCATION "${URL}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; --- install -----------------------------------------------------------------

Section "${NAME}" SecCore
  SectionIn RO
  SetOverwrite on

  !include "${FILES_NSH}"

  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKLM "Software\${NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${NAME}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\sterna.exe"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKLM "${UNINST_KEY}" "URLInfoAbout" "${URL}"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1

  ; Add > Remove Programs shows nothing at all in the size column without
  ; this, which reads as a half-registered program.
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" "$0"

  ; One shortcut, not a folder with a shortcut and an uninstaller in it: the
  ; Start menu has had its own uninstall route since Windows 8, and a folder
  ; holding one item is a folder the user has to open every time.
  CreateShortcut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\sterna.exe"
SectionEnd

Section "Desktop shortcut" SecDesktop
  CreateShortcut "$DESKTOP\${NAME}.lnk" "$INSTDIR\sterna.exe"
SectionEnd

; Off by default, which is upstream's answer too (`teraterm.iss:285`, a task
; flagged `unchecked`) and a better one than it looks: `.ttl` is also Turtle,
; the RDF serialisation, and on a machine that edits those this extension is
; already spoken for. So this registers through `OpenWithProgids` rather than
; by writing `.ttl`'s default value — the additive form, which adds Sterna to
; Open with and to the Windows 8+ "how do you want to open this" list without
; taking the extension away from whatever already holds it.
;
; **The command is `sterna.exe /M=`, not `ttpmacro.exe`, and upstream's is the
; other way round** (`teraterm.iss:225`). Upstream's `ttpmacro.exe` is the
; interpreter and runs the script in its own process; here the interpreter is
; inside the window and `ttpmacro` is a *client* of one, so the literal
; registration fails on a machine with no window open — which is most of what
; double-clicking a file means. It would fail invisibly, too: `ttpmacro` is a
; console-subsystem program, so Explorer would give it a console window that
; flashes and is gone with the diagnostic in it. `/M=` is upstream's own
; `ttermpro /M=script.ttl`, and what it costs is that a macro opens a *new*
; window rather than running in one that is already up.
Section /o "Associate .ttl macro files" SecAssoc
  WriteRegStr HKLM "Software\Classes\${PROGID}" "" "${NAME} macro"
  WriteRegStr HKLM "Software\Classes\${PROGID}\DefaultIcon" "" '"$INSTDIR\sterna.exe",0'
  WriteRegStr HKLM "Software\Classes\${PROGID}\shell\open\command" "" \
    '"$INSTDIR\sterna.exe" /M="%1"'
  WriteRegStr HKLM "Software\Classes\.ttl\OpenWithProgids" "${PROGID}" ""

  ; Explorer caches associations and does not re-read the registry on its own,
  ; so without this the entry appears at the next login rather than now.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCore} \
    "The terminal, its core, the two control-socket clients (ttctl and ttpmacro) and the language files."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} \
    "A shortcut on the desktop as well as in the Start menu."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAssoc} \
    "Offer Sterna in Explorer's Open with menu for .ttl macro files. It does not take the extension over from another program."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Function StartSterna
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\sterna.exe"'
FunctionEnd

Function .onInit
  ; The components page is the ordinary way to turn the association on, and a
  ; silent install has no page — so `/S /ASSOC` is how a deployment script asks
  ; for it. Upstream's installer takes `/TASKS=macroassoc` for the same reason;
  ; Inno supplies that itself, and NSIS does not.
  ${GetParameters} $R8

  ; A signed in-app update starts this installer with the old process id. Wait
  ; before running its uninstaller: Windows cannot delete an executable which
  ; is still running, and leaving the old `sterna.exe` beside new DLLs produces
  ; a loader failure before `main`. OpenProcess failing means the process
  ; already ended. A timeout aborts before touching the installed files.
  ClearErrors
  ${GetOptions} $R8 "/UPDATEPID=" $R7
  ${IfNot} ${Errors}
  ${AndIf} $R7 != ""
    StrCpy $UpdateInstaller 1
    System::Call 'kernel32::OpenProcess(i 0x00100000, i 0, i $R7) p .r0'
    ${If} $0 != 0
      System::Call 'kernel32::WaitForSingleObject(p r0, i 120000) i .r1'
      System::Call 'kernel32::CloseHandle(p r0)'
      ${If} $1 != 0
        MessageBox MB_OK|MB_ICONSTOP \
          "Sterna did not close in time. The existing installation was not changed."
        Abort
      ${EndIf}
    ${EndIf}
  ${EndIf}
  ClearErrors
  ${GetOptions} $R8 "/RESTART" $R9
  ${IfNot} ${Errors}
    StrCpy $UpdateRestart 1
  ${EndIf}
  ClearErrors

  ClearErrors
  ${GetOptions} $R8 "/ASSOC" $R9
  ${IfNot} ${Errors}
    !insertmacro SelectSection ${SecAssoc}
  ${EndIf}
  ClearErrors

  ; Upgrade in place over an older install and the files that version had and
  ; this one does not are left behind — which for a Qt DLL is not inert: the
  ; loader finds the stale one first and the program dies before `main` with a
  ; missing-entry-point box naming a symbol nobody has heard of. So the old
  ; uninstaller runs first, and `_?=` keeps it in place long enough to be
  ; waited on rather than having it copy itself to the temp directory and
  ; return immediately.
  ReadRegStr $R0 HKLM "${UNINST_KEY}" "UninstallString"
  ReadRegStr $R1 HKLM "Software\${NAME}" "InstallDir"
  ${If} $R0 != ""
  ${AndIf} $R1 != ""
  ${AndIf} ${FileExists} "$R1\uninstall.exe"
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "${NAME} is already installed in $R1.$\n$\nRemove that installation before continuing?" \
      /SD IDYES IDNO keep
    ClearErrors
    ExecWait '"$R1\uninstall.exe" /S _?=$R1'
    Delete "$R1\uninstall.exe"
    RMDir "$R1"
  keep:
  ${EndIf}
FunctionEnd

Function .onInstSuccess
  ${If} $UpdateRestart == 1
    ; Same de-elevation route as the finish page: the updater installer is
    ; elevated, while the terminal must read the desktop user's AppData.
    Exec '"$WINDIR\explorer.exe" "$INSTDIR\sterna.exe"'
  ${EndIf}
  ${If} $UpdateInstaller == 1
    ; The downloaded setup is in the user's temp directory. Its executing file
    ; is locked, so queue deletion for reboot rather than accumulating one per
    ; release forever.
    Delete /REBOOTOK "$EXEPATH"
  ${EndIf}
FunctionEnd

; --- uninstall ---------------------------------------------------------------

Section "Uninstall"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"

  ; Every file by name and every directory with a plain RMDir, which refuses a
  ; directory that is not empty — so anything the user put in the program
  ; folder survives, and so does the folder. `RMDir /r "$INSTDIR"` is the
  ; alternative and it is a recursive delete of a path typed into the
  ; directory page.
  !include "${UNINSTALL_NSH}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\${NAME}"

  ; Unconditional, because the uninstaller does not know which sections ran and
  ; removing a value that was never written is a no-op. `/ifempty` on the two
  ; extension keys is what keeps this from taking `.ttl` away from a program
  ; that owns it: whichever of them somebody else has a value in stays, and
  ; one we created and emptied goes. Upstream leaves `.ttl` behind outright
  ; (`teraterm.iss:262`, a comment saying so).
  DeleteRegKey HKLM "Software\Classes\${PROGID}"
  DeleteRegValue HKLM "Software\Classes\.ttl\OpenWithProgids" "${PROGID}"
  DeleteRegKey /ifempty HKLM "Software\Classes\.ttl\OpenWithProgids"
  DeleteRegKey /ifempty HKLM "Software\Classes\.ttl"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  ; `sterna.ini` is deliberately not touched. It is under the user's own
  ; AppData rather than in the program folder, it is one file per user on a
  ; machine that may have several, and an uninstall that is really an upgrade
  ; would take the user's settings with it.
SectionEnd
