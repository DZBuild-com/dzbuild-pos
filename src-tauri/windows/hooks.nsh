; NSIS hooks for the DZBuild POS installer (tauri.conf.json > bundle.windows.nsis.installerHooks).
; ${MAINBINARYNAME} is dzbuild-pos, ${BUNDLEID} dev.dzbuild.pos; $UpdateMode is 1 when the
; updater runs the setup with /UPDATE.

; NFR-002 (online setup <= 15 MB): Tauri includes this file right after its `SetCompressor /SOLID
; lzma` and before any file, the one place NSIS takes a compressor setting. NSIS's default 8 MB
; LZMA window cannot reach back across the ~50 MB app, so repeated code is stored again; 64 MB
; covers the whole payload. The setup needs that much memory while it unpacks, and the build PC
; about ten times more. Warning 8026 ("data already compressed in compress whole mode") is wrong
; for a solid setup, which is compressed in one pass when it is written: the 0.8.0 setup is
; 15,777,503 bytes with this line and 16,093,773 with 8 MB here.
!pragma warning push
!pragma warning disable 8026
SetCompressorDictSize 64
!pragma warning pop

!macro NSIS_HOOK_POSTINSTALL
  ; PLT-008, spec §4.6: the main PC (LAN host role, M5) listens on TCP 7345 and finds the tills
  ; over mDNS (UDP 5353). Allow both on private networks only, for this program only, under the
  ; names pos_server::firewall checks (RULE_TCP, RULE_MDNS; install_commands are these lines).
  ; Recreated on every install and update, so a moved $INSTDIR never leaves a stale rule.
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="DZBuild POS LAN"'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="DZBuild POS LAN" dir=in action=allow protocol=TCP localport=7345 profile=private program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="DZBuild POS mDNS"'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="DZBuild POS mDNS" dir=in action=allow protocol=UDP localport=5353 profile=private program="$INSTDIR\${MAINBINARYNAME}.exe" enable=yes'
  Pop $0
  ; PLT-017: Windows Error Reporting keeps the last 5 minidumps of the app on this PC. Nothing
  ; is uploaded; the Diagnostic screen only counts them (diagnostics::crash_dumps_dir).
  SetRegView 64
  WriteRegExpandStr HKLM "SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\${MAINBINARYNAME}.exe" "DumpFolder" "%LOCALAPPDATA%\${BUNDLEID}\crashdumps"
  WriteRegDWORD HKLM "SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\${MAINBINARYNAME}.exe" "DumpCount" 5
  WriteRegDWORD HKLM "SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\${MAINBINARYNAME}.exe" "DumpType" 1
!macroend

; The confirm page's box ("Supprimer aussi le cache d'affichage…", customLanguageFiles) removes
; only what can be rebuilt: %LOCALAPPDATA%\dev.dzbuild.pos (WebView2 cache, crash dumps).
; %APPDATA%\dev.dzbuild.pos (pos.db with the sales and the hash-chained journal, the backups,
; the logs) always survives an uninstall: erasing it would be a sales-deletion path the product
; must never ship (LF 2026 preservation). Removing everything is a documented manual step
; (help page "Supprimer toutes les données", remove-data). This hook runs at the start of the
; uninstall section, after the box was read: it keeps the answer for NSIS_HOOK_POSTUNINSTALL
; and hands Tauri's own deletion test (which would erase %APPDATA% too) a 0.
Var DisplayCacheBoxState

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $DisplayCacheBoxState $DeleteAppDataCheckboxState
  StrCpy $DeleteAppDataCheckboxState 0
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="DZBuild POS LAN"'
    Pop $0
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="DZBuild POS mDNS"'
    Pop $0
    SetRegView 64
    DeleteRegKey HKLM "SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\${MAINBINARYNAME}.exe"
  ${EndIf}
  ; The cache goes here rather than in NSIS_HOOK_PREUNINSTALL: by now the uninstaller has closed
  ; the app, so WebView2 no longer holds its files open.
  ${If} $DisplayCacheBoxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
    SetShellVarContext all
  ${EndIf}
!macroend
