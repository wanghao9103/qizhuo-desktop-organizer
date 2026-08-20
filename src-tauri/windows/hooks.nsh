!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$DESKTOP\qizhuo.lnk" "$INSTDIR\qizhuo.exe" "" "$INSTDIR\qizhuo.exe" 0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$DESKTOP\qizhuo.lnk"
!macroend

